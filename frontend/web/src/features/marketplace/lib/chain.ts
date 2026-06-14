// Chain-side helpers for the marketplace purchase flow (Mantle).
//
// Network is selected at build time via `VITE_MARKETPLACE_NETWORK`
// (default "sepolia" = Mantle Sepolia 5003; "mainnet" = Mantle 5000). The
// chain + USDC EIP-712 domain come from `networkConfig`/`activeConfig`.
//
// Contract addresses are NEVER hardcoded in the bundle — they're discovered
// from `GET /api/marketplace/status` (`contracts` block) and cached for the
// session. All USDC amounts are integer 6dp bigints; string/decimal
// conversion happens only at the relay-body edge.
//
// The EIP-3009 `transferWithAuthorization` typed data here must match the
// active network USDC's EIP-712 domain exactly (mainnet USDC.e: name
// "USD Coin", version "2", chainId 5000; testnet test USDC: "USD Coin
// (xvn test)", version "1", chainId 5003), verifyingContract = the USDC
// address. The typehash field order (from, to, value, validAfter,
// validBefore, nonce) is normative — reordering changes the digest and the
// relay tx reverts.

import {
  createPublicClient,
  createWalletClient,
  custom,
  defineChain,
  http,
  parseSignature,
  type Address,
  type Chain,
  type Hex,
  type PublicClient,
  type WalletClient,
} from "viem";
import { apiFetch } from "@/api/client";
import { WALLET_STORAGE_KEY } from "./wallet";

// ---------------------------------------------------------------------------
// Chain
// ---------------------------------------------------------------------------

export const mantleSepolia = defineChain({
  id: 5003,
  name: "Mantle Sepolia",
  nativeCurrency: { name: "Mantle", symbol: "MNT", decimals: 18 },
  rpcUrls: {
    default: { http: ["https://rpc.sepolia.mantle.xyz"] },
  },
  blockExplorers: {
    default: {
      name: "Mantle Sepolia Explorer",
      url: "https://explorer.sepolia.mantle.xyz",
    },
  },
});

export const mantleMainnet = defineChain({
  id: 5000,
  name: "Mantle",
  nativeCurrency: { name: "Mantle", symbol: "MNT", decimals: 18 },
  rpcUrls: {
    default: { http: ["https://rpc.mantle.xyz"] },
  },
  blockExplorers: {
    default: { name: "Mantle Explorer", url: "https://explorer.mantle.xyz" },
  },
});

export type MarketplaceNetwork = "sepolia" | "mainnet";

export interface MarketplaceNetworkConfig {
  chain: Chain;
  /** chainId as the hex string wallets expect (wallet_switchEthereumChain). */
  hex: `0x${string}`;
  /**
   * The network USDC's EIP-712 domain name/version, for the EIP-3009
   * `transferWithAuthorization` signature. Verified on-chain:
   * mainnet USDC.e (0x09Bc…0dF9) is "USD Coin"/"2"; the testnet test USDC
   * is "USD Coin (xvn test)"/"1".
   */
  usdcDomain: { name: string; version: string };
}

/** Pure: the chain + USDC-domain config for a network. */
export function networkConfig(
  network: MarketplaceNetwork,
): MarketplaceNetworkConfig {
  return network === "mainnet"
    ? {
        chain: mantleMainnet,
        hex: "0x1388", // 5000
        usdcDomain: { name: "USD Coin", version: "2" },
      }
    : {
        chain: mantleSepolia,
        hex: "0x138b", // 5003
        usdcDomain: { name: "USD Coin (xvn test)", version: "1" },
      };
}

/**
 * Build-time network selector. Defaults to "sepolia" unless
 * `VITE_MARKETPLACE_NETWORK=mainnet`. MUST stay the literal
 * `import.meta.env.VITE_…` expression so Vite's define replacement rewrites it.
 */
function resolveNetwork(): MarketplaceNetwork {
  return import.meta.env.VITE_MARKETPLACE_NETWORK === "mainnet"
    ? "mainnet"
    : "sepolia";
}

/** The active marketplace network + its chain/USDC-domain config. */
export const activeNetwork: MarketplaceNetwork = resolveNetwork();
export const activeConfig = networkConfig(activeNetwork);
/** The viem chain for the active network (5003 testnet or 5000 mainnet). */
export const activeChain = activeConfig.chain;

/** Active chain id as the hex string wallets expect. */
export const MANTLE_ACTIVE_HEX = activeConfig.hex;
/** @deprecated kept for back-compat; use {@link MANTLE_ACTIVE_HEX}. */
export const MANTLE_SEPOLIA_HEX = "0x138b";

/**
 * Network slug stamped onto `TxRef.network` / on-chain NFT metadata. Drives the
 * block-explorer choice in `TxChip` ("mantle" → explorer.mantle.xyz;
 * "mantle-sepolia" → explorer.sepolia.mantle.xyz). Build-time constant; mirrors
 * {@link activeNetwork}. Replaces the old hardcoded "mantle-sepolia" literals so
 * a mainnet build never renders Sepolia explorer links.
 */
export const activeNetworkSlug: string =
  activeNetwork === "mainnet" ? "mantle" : "mantle-sepolia";

/**
 * Call-time mainnet check. Reads the build-time `VITE_MARKETPLACE_NETWORK`
 * literal (so Vite's define-replacement applies, and tests can `vi.stubEnv` it).
 * Used by the testnet badge/banner so they stay honest on a mainnet build —
 * a "purchases are simulated" notice on mainnet would be false and unsafe.
 */
export function isMainnetNetwork(): boolean {
  return import.meta.env.VITE_MARKETPLACE_NETWORK === "mainnet";
}

// ---------------------------------------------------------------------------
// Contract discovery (cached from /api/marketplace/status)
// ---------------------------------------------------------------------------

/** Mirrors `ContractsOut` in marketplace_read.rs (each null when unset). */
export interface MarketplaceContracts {
  marketplace: Address;
  usdc: Address;
  license_token: string | null;
  listing_registry: string | null;
  identity_registry: string | null;
}

interface StatusOut {
  contracts: {
    marketplace: string | null;
    usdc: string | null;
    license_token: string | null;
    listing_registry: string | null;
    identity_registry: string | null;
  };
  /** Public IPFS read gateway base (no trailing slash, no `/ipfs`). */
  public_gateway?: string | null;
}

let contractsCache: MarketplaceContracts | null = null;

/**
 * Vendor-neutral fallback public read gateway, mirroring the backend default
 * (`DEFAULT_PUBLIC_GATEWAY` in marketplace_read.rs). `dweb.link` is the
 * IPFS-canonical public gateway, not a vendor product — used only when the
 * status route does not surface a configured gateway.
 */
export const DEFAULT_PUBLIC_GATEWAY = "https://dweb.link";

let publicGatewayCache: string | null = null;

/**
 * The public IPFS read gateway base for "open bundle" links, config-driven
 * from `GET /api/marketplace/status` (`public_gateway`). Returns a base with
 * no trailing slash; callers build `${base}/ipfs/${cid}`. Falls back to the
 * vendor-neutral default when the status route lacks the field or is
 * unreachable (never throws — this only powers a convenience link).
 */
export async function getPublicGateway(): Promise<string> {
  if (publicGatewayCache) return publicGatewayCache;
  let gateway = DEFAULT_PUBLIC_GATEWAY;
  try {
    const status = await apiFetch<StatusOut>("/api/marketplace/status");
    const g = status.public_gateway?.trim();
    if (g) gateway = g.replace(/\/+$/, "");
  } catch {
    // Status unreachable — keep the neutral default.
  }
  publicGatewayCache = gateway;
  return gateway;
}

/** Test-only: clear the cached public gateway. */
export function __resetPublicGatewayCacheForTest(): void {
  publicGatewayCache = null;
}

/**
 * Fetch (once per session) the marketplace contract address book.
 * Throws when the marketplace or USDC address is not configured on the
 * backend — purchase flows cannot proceed without both.
 */
export async function getContracts(): Promise<MarketplaceContracts> {
  if (contractsCache) return contractsCache;
  const status = await apiFetch<StatusOut>("/api/marketplace/status");
  const c = status.contracts;
  if (!c.marketplace || !c.usdc) {
    throw new Error(
      "Marketplace contracts not configured on the backend (marketplace/usdc address missing).",
    );
  }
  contractsCache = {
    marketplace: c.marketplace as Address,
    usdc: c.usdc as Address,
    license_token: c.license_token,
    listing_registry: c.listing_registry,
    identity_registry: c.identity_registry,
  };
  return contractsCache;
}

/** Test-only: clear the cached contract address book. */
export function __resetContractsCacheForTest(): void {
  contractsCache = null;
}

// ---------------------------------------------------------------------------
// Clients
// ---------------------------------------------------------------------------

export function publicClient(): PublicClient {
  return createPublicClient({ chain: activeChain, transport: http() });
}

export function walletClient(): WalletClient {
  if (!window.ethereum) {
    throw new Error(
      "MetaMask (or compatible wallet) not detected. Install from metamask.io.",
    );
  }
  return createWalletClient({
    chain: activeChain,
    transport: custom(window.ethereum),
  });
}

async function connectedAccount(): Promise<Address> {
  const [account] = await walletClient().getAddresses();
  if (!account) {
    throw new Error("No wallet account connected.");
  }
  return account;
}

/**
 * The connected wallet address, or null when no wallet / not connected.
 * Prefers the `useWallet` localStorage key (set on explicit connect),
 * falling back to `eth_accounts` (already-authorized accounts; does not
 * prompt). Never throws — callers turn null into a typed error.
 */
export async function currentAddress(): Promise<Address | null> {
  if (!window.ethereum) return null;
  const stored = localStorage.getItem(WALLET_STORAGE_KEY);
  if (stored) return stored as Address;
  try {
    const accounts = (await window.ethereum.request({
      method: "eth_accounts",
    })) as string[];
    return (accounts?.[0] as Address | undefined) ?? null;
  } catch {
    return null;
  }
}

/**
 * Ensure the wallet is on the active Mantle network (Sepolia 5003 by default,
 * or mainnet 5000 when `VITE_MARKETPLACE_NETWORK=mainnet`): check `eth_chainId`,
 * attempt `wallet_switchEthereumChain`, and on error 4902 (chain unknown to the
 * wallet) add the chain first, then switch. (Name kept for back-compat.)
 */
export async function ensureMantleSepolia(): Promise<void> {
  if (!window.ethereum) {
    throw new Error(
      "MetaMask (or compatible wallet) not detected. Install from metamask.io.",
    );
  }
  const chainId = (await window.ethereum.request({
    method: "eth_chainId",
  })) as string;
  if (chainId?.toLowerCase() === MANTLE_ACTIVE_HEX) return;
  try {
    await window.ethereum.request({
      method: "wallet_switchEthereumChain",
      params: [{ chainId: MANTLE_ACTIVE_HEX }],
    });
  } catch (err) {
    const code = (err as { code?: number })?.code;
    if (code !== 4902) throw err;
    const blockExplorerUrl = activeChain.blockExplorers?.default?.url;
    await window.ethereum.request({
      method: "wallet_addEthereumChain",
      params: [
        {
          chainId: MANTLE_ACTIVE_HEX,
          chainName: activeChain.name,
          nativeCurrency: activeChain.nativeCurrency,
          rpcUrls: activeChain.rpcUrls.default.http,
          ...(blockExplorerUrl
            ? { blockExplorerUrls: [blockExplorerUrl] }
            : {}),
        },
      ],
    });
    await window.ethereum.request({
      method: "wallet_switchEthereumChain",
      params: [{ chainId: MANTLE_ACTIVE_HEX }],
    });
  }
}

// ---------------------------------------------------------------------------
// USDC reads/writes + marketplace buy
// ---------------------------------------------------------------------------

const ERC20_READS_ABI = [
  {
    type: "function",
    name: "balanceOf",
    stateMutability: "view",
    inputs: [{ name: "account", type: "address" }],
    outputs: [{ name: "", type: "uint256" }],
  },
  {
    type: "function",
    name: "allowance",
    stateMutability: "view",
    inputs: [
      { name: "owner", type: "address" },
      { name: "spender", type: "address" },
    ],
    outputs: [{ name: "", type: "uint256" }],
  },
] as const;

const ERC20_APPROVE_ABI = [
  {
    type: "function",
    name: "approve",
    stateMutability: "nonpayable",
    inputs: [
      { name: "spender", type: "address" },
      { name: "value", type: "uint256" },
    ],
    outputs: [{ name: "", type: "bool" }],
  },
] as const;

const USDC_FAUCET_ABI = [
  {
    type: "function",
    name: "faucet",
    stateMutability: "nonpayable",
    inputs: [{ name: "amount", type: "uint256" }],
    outputs: [],
  },
] as const;

const MARKETPLACE_BUY_ABI = [
  {
    type: "function",
    name: "buy",
    stateMutability: "nonpayable",
    inputs: [
      { name: "listingId", type: "uint256" },
      { name: "recipient", type: "address" },
    ],
    outputs: [],
  },
] as const;

/** USDC balance of `addr`, integer 6dp. */
export async function usdcBalance(addr: Address): Promise<bigint> {
  const { usdc } = await getContracts();
  return publicClient().readContract({
    address: usdc,
    abi: ERC20_READS_ABI,
    functionName: "balanceOf",
    args: [addr],
  });
}

/** USDC allowance from `owner` to the marketplace, integer 6dp. */
export async function usdcAllowance(owner: Address): Promise<bigint> {
  const { usdc, marketplace } = await getContracts();
  return publicClient().readContract({
    address: usdc,
    abi: ERC20_READS_ABI,
    functionName: "allowance",
    args: [owner, marketplace],
  });
}

async function writeAndWait(
  write: (account: Address) => Promise<Hex>,
): Promise<Hex> {
  const account = await connectedAccount();
  const hash = await write(account);
  await publicClient().waitForTransactionReceipt({ hash });
  return hash;
}

/** Mint test USDC to the connected account. `amount6` is integer 6dp. */
export async function faucetUsdc(amount6: bigint): Promise<Hex> {
  const { usdc } = await getContracts();
  return writeAndWait((account) =>
    walletClient().writeContract({
      address: usdc,
      abi: USDC_FAUCET_ABI,
      functionName: "faucet",
      args: [amount6],
      account,
      chain: activeChain,
    }),
  );
}

/** Approve the marketplace to spend `amount6` USDC (integer 6dp). */
export async function approveUsdc(amount6: bigint): Promise<Hex> {
  const { usdc, marketplace } = await getContracts();
  return writeAndWait((account) =>
    walletClient().writeContract({
      address: usdc,
      abi: ERC20_APPROVE_ABI,
      functionName: "approve",
      args: [marketplace, amount6],
      account,
      chain: activeChain,
    }),
  );
}

/** Direct (approve-path) purchase: `Marketplace.buy(listingId, recipient)`. */
export async function buyDirect(
  listingId: bigint,
  recipient: Address,
): Promise<Hex> {
  const { marketplace } = await getContracts();
  return writeAndWait((account) =>
    walletClient().writeContract({
      address: marketplace,
      abi: MARKETPLACE_BUY_ABI,
      functionName: "buy",
      args: [listingId, recipient],
      account,
      chain: activeChain,
    }),
  );
}

// ---------------------------------------------------------------------------
// EIP-3009 transferWithAuthorization signing
// ---------------------------------------------------------------------------

export interface TransferAuthMessage {
  from: Address;
  to: Address;
  value: bigint;
  validAfter: bigint;
  validBefore: bigint;
  nonce: Hex;
}

/** Relay body for `POST /api/marketplace/buy`'s `authorization` field. */
export interface RelayAuthorization {
  from: Address;
  to: Address;
  /** Decimal 6dp string (serde-friendly; avoids JS number precision). */
  value: string;
  valid_after: number;
  valid_before: number;
  nonce: Hex;
  v: number;
  r: Hex;
  s: Hex;
}

/** Random 32-byte hex nonce via `crypto.getRandomValues`. */
export function randomNonce(): Hex {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return `0x${Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("")}`;
}

/**
 * Pure builder for the EIP-3009 typed data. Field names and order are
 * normative (they hash into the typehash); do not reorder.
 */
export function buildTransferAuthTypedData(params: {
  from: Address;
  to: Address;
  usdc: Address;
  value: bigint;
  validAfter: bigint;
  validBefore: bigint;
  nonce: Hex;
}) {
  return {
    domain: {
      name: activeConfig.usdcDomain.name,
      version: activeConfig.usdcDomain.version,
      chainId: activeChain.id,
      verifyingContract: params.usdc,
    },
    types: {
      TransferWithAuthorization: [
        { name: "from", type: "address" },
        { name: "to", type: "address" },
        { name: "value", type: "uint256" },
        { name: "validAfter", type: "uint256" },
        { name: "validBefore", type: "uint256" },
        { name: "nonce", type: "bytes32" },
      ],
    },
    primaryType: "TransferWithAuthorization",
    message: {
      from: params.from,
      to: params.to,
      value: params.value,
      validAfter: params.validAfter,
      validBefore: params.validBefore,
      nonce: params.nonce,
    },
  } as const;
}

/**
 * Pure: split a 65-byte signature into r/s/v (v normalized to 27/28 —
 * wallets may return yParity 0/1 in the trailing byte) and assemble the
 * relay `authorization` body.
 */
export function relayBodyFromSignature(
  message: TransferAuthMessage,
  signature: Hex,
): RelayAuthorization {
  const { r, s, v, yParity } = parseSignature(signature);
  const vNorm =
    v !== undefined && v >= 27n ? Number(v) : (yParity ?? 0) + 27;
  return {
    from: message.from,
    to: message.to,
    value: message.value.toString(),
    valid_after: Number(message.validAfter),
    valid_before: Number(message.validBefore),
    nonce: message.nonce,
    v: vNorm,
    r,
    s,
  };
}

/**
 * Sign an EIP-3009 transfer authorization paying the marketplace contract
 * `valueUsdc6` (integer 6dp) from `from`, valid for `validSecs` (default
 * 1h). Returns the relay `authorization` body for `POST /api/marketplace/buy`.
 */
export async function signTransferAuthorization(params: {
  from: Address;
  valueUsdc6: bigint;
  validSecs?: number;
}): Promise<RelayAuthorization> {
  const { from, valueUsdc6, validSecs = 3600 } = params;
  const { usdc, marketplace } = await getContracts();
  const message: TransferAuthMessage = {
    from,
    to: marketplace,
    value: valueUsdc6,
    validAfter: 0n,
    validBefore: BigInt(Math.floor(Date.now() / 1000) + validSecs),
    nonce: randomNonce(),
  };
  const typedData = buildTransferAuthTypedData({ ...message, usdc });
  const signature = await walletClient().signTypedData({
    account: from,
    ...typedData,
  });
  return relayBodyFromSignature(message, signature);
}
