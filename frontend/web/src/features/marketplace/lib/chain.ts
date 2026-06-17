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
// Runtime network selection (backend = source of truth)
// ---------------------------------------------------------------------------
//
// The backend reports its configured chain via `GET /api/marketplace/status`
// (`network.chain_id`, set from `XVN_CHAIN_ID`). We resolve the active network
// from THAT at runtime — so one prebuilt bundle works on testnet or mainnet
// purely by the backend env, and the build-time `VITE_MARKETPLACE_NETWORK` is
// only a fallback when the backend reports no chain. This removes the silent
// build/runtime mismatch that would otherwise sign EIP-3009 buys with the wrong
// USDC EIP-712 domain (→ on-chain revert).

/** A fully resolved network: chain + wallet-switch hex + USDC domain + slug. */
export interface ResolvedNetwork extends MarketplaceNetworkConfig {
  /** Receipt/explorer slug ("mantle" | "mantle-sepolia"). */
  slug: string;
}

/** Chain id → resolved network. The runtime selector maps the backend's
 *  `chain_id` through this; unknown ids fall through to the build-time default. */
const CHAIN_BY_ID: Record<number, ResolvedNetwork> = {
  5000: { ...networkConfig("mainnet"), slug: "mantle" },
  5003: { ...networkConfig("sepolia"), slug: "mantle-sepolia" },
};

/** Build-time fallback used when the backend reports no chain (or is
 *  unreachable for the cosmetic resolver). Derived from VITE_MARKETPLACE_NETWORK. */
const BUILD_TIME_NETWORK: ResolvedNetwork = {
  ...activeConfig,
  slug: activeNetworkSlug,
};

// ---------------------------------------------------------------------------
// Contract discovery + network (one cached read of /api/marketplace/status)
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
  /** The backend's configured chain (from `XVN_CHAIN_ID`). Absent/null when no
   *  chain is configured — the SPA then keeps its build-time default. */
  network?: { chain_id: number; network: string; explorer_base?: string } | null;
}

/** Parsed once per session from `/api/marketplace/status`. `network` is the
 *  RESOLVED network (non-null only for a chain id we support); `chainId` is the
 *  RAW backend chain id (null when no chain is configured) — kept distinct so
 *  callers can tell "backend reports no chain" (safe to default) from "backend
 *  reports a chain id we don't recognise" (a misconfig the signing path must
 *  reject, not silently mis-sign). */
interface ChainContext {
  contracts: MarketplaceContracts | null;
  network: ResolvedNetwork | null;
  chainId: number | null;
}

let chainContextCache: ChainContext | null = null;
let chainContextInflight: Promise<ChainContext> | null = null;

/**
 * Fetch (once per session) the backend's marketplace status and project it to a
 * {contracts, network, chainId} context. Throws only when the status route is
 * unreachable — NOT when contracts/network are merely unconfigured (callers
 * decide). Backs {@link getContracts} and {@link getActiveNetworkConfig}, so
 * contract addresses and the chain come from the SAME status read (they can
 * never disagree). De-dupes concurrent first-callers via an in-flight promise so
 * a busy render fires one request, not several. (The public gateway / Lit config
 * keep their own caches — they read the same route but aren't on this path.)
 */
export async function getChainContext(): Promise<ChainContext> {
  if (chainContextCache) return chainContextCache;
  if (chainContextInflight) return chainContextInflight;
  chainContextInflight = (async () => {
    const status = await apiFetch<StatusOut>("/api/marketplace/status");
    const c = status.contracts;
    const contracts: MarketplaceContracts | null =
      c?.marketplace && c?.usdc
        ? {
            marketplace: c.marketplace as Address,
            usdc: c.usdc as Address,
            license_token: c.license_token,
            listing_registry: c.listing_registry,
            identity_registry: c.identity_registry,
          }
        : null;
    const chainId = status.network?.chain_id ?? null;
    const network = chainId != null ? (CHAIN_BY_ID[chainId] ?? null) : null;
    chainContextCache = { contracts, network, chainId };
    return chainContextCache;
  })();
  try {
    return await chainContextInflight;
  } finally {
    chainContextInflight = null;
  }
}

/**
 * STRICT runtime network resolver for the signing/buy path. Returns the
 * backend's network when known; the build-time default ONLY when the backend
 * reports NO chain. When the backend reports a chain id we don't support, this
 * THROWS rather than signing with a guessed (build-time) USDC EIP-712 domain
 * that would revert on-chain. Also propagates status-fetch errors (no silent
 * fallback). Cheap after the first call (shared cache).
 */
export async function getActiveNetworkConfig(): Promise<ResolvedNetwork> {
  const { network, chainId } = await getChainContext();
  if (network) return network;
  if (chainId != null) {
    throw new Error(
      `Backend reports unsupported marketplace chain id ${chainId}; the SPA ` +
        `cannot resolve its USDC EIP-712 signing domain. Set XVN_CHAIN_ID to a ` +
        `supported chain (5000 Mantle mainnet, 5003 Mantle Sepolia).`,
    );
  }
  return BUILD_TIME_NETWORK;
}

/**
 * LENIENT resolver for cosmetic surfaces (badges, explorer-link slugs, the
 * sealed-tier read RPC). Never throws — falls back to the build-time default on
 * any error (including an unsupported backend chain) so the UI degrades
 * gracefully.
 */
export async function getActiveNetworkConfigOrDefault(): Promise<ResolvedNetwork> {
  try {
    return await getActiveNetworkConfig();
  } catch {
    return BUILD_TIME_NETWORK;
  }
}

/**
 * The RAW chain id the backend reports (null when no chain is configured or the
 * status route is unreachable). Used by the mismatch guard so it can flag a
 * build/backend chain disagreement even for a chain id the SPA can't resolve.
 */
export async function getBackendChainId(): Promise<number | null> {
  try {
    return (await getChainContext()).chainId;
  } catch {
    return null;
  }
}

/** Test-only: clear the shared status cache (contracts + network + chainId). */
export function __resetNetworkCacheForTest(): void {
  chainContextCache = null;
  chainContextInflight = null;
}

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
  const { contracts } = await getChainContext();
  if (!contracts) {
    throw new Error(
      "Marketplace contracts not configured on the backend (marketplace/usdc address missing).",
    );
  }
  return contracts;
}

/** Test-only: clear the cached contract address book (shared with the network
 *  cache — both are projected from one `/api/marketplace/status` read). */
export function __resetContractsCacheForTest(): void {
  chainContextCache = null;
}

// ---------------------------------------------------------------------------
// Clients
// ---------------------------------------------------------------------------

/** Public (read) client. Pass the resolved chain (from
 *  {@link getActiveNetworkConfig}) so reads hit the backend-selected chain's RPC;
 *  defaults to the build-time chain for callers that don't care. */
export function publicClient(chain: Chain = activeChain): PublicClient {
  return createPublicClient({ chain, transport: http() });
}

/** Wallet (write/sign) client. Pass the resolved chain so writes target the
 *  backend-selected chain; defaults to the build-time chain. */
export function walletClient(chain: Chain = activeChain): WalletClient {
  if (!window.ethereum) {
    throw new Error(
      "MetaMask (or compatible wallet) not detected. Install from metamask.io.",
    );
  }
  return createWalletClient({
    chain,
    transport: custom(window.ethereum),
  });
}

async function connectedAccount(): Promise<Address> {
  // Address retrieval is chain-independent; the build-time default client is fine.
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
  // Target the BACKEND-selected chain, not the build-time default — otherwise a
  // prebuilt (sepolia) bundle on a mainnet backend would switch the wallet to
  // the wrong chain.
  const net = await getActiveNetworkConfig();
  const targetHex = net.hex;
  const chainId = (await window.ethereum.request({
    method: "eth_chainId",
  })) as string;
  if (chainId?.toLowerCase() === targetHex) return;
  try {
    await window.ethereum.request({
      method: "wallet_switchEthereumChain",
      params: [{ chainId: targetHex }],
    });
  } catch (err) {
    const code = (err as { code?: number })?.code;
    if (code !== 4902) throw err;
    const blockExplorerUrl = net.chain.blockExplorers?.default?.url;
    await window.ethereum.request({
      method: "wallet_addEthereumChain",
      params: [
        {
          chainId: targetHex,
          chainName: net.chain.name,
          nativeCurrency: net.chain.nativeCurrency,
          rpcUrls: net.chain.rpcUrls.default.http,
          ...(blockExplorerUrl
            ? { blockExplorerUrls: [blockExplorerUrl] }
            : {}),
        },
      ],
    });
    await window.ethereum.request({
      method: "wallet_switchEthereumChain",
      params: [{ chainId: targetHex }],
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
  const net = await getActiveNetworkConfig();
  return publicClient(net.chain).readContract({
    address: usdc,
    abi: ERC20_READS_ABI,
    functionName: "balanceOf",
    args: [addr],
  });
}

/** USDC allowance from `owner` to the marketplace, integer 6dp. */
export async function usdcAllowance(owner: Address): Promise<bigint> {
  const { usdc, marketplace } = await getContracts();
  const net = await getActiveNetworkConfig();
  return publicClient(net.chain).readContract({
    address: usdc,
    abi: ERC20_READS_ABI,
    functionName: "allowance",
    args: [owner, marketplace],
  });
}

async function writeAndWait(
  write: (account: Address) => Promise<Hex>,
  chain: Chain = activeChain,
): Promise<Hex> {
  const account = await connectedAccount();
  const hash = await write(account);
  await publicClient(chain).waitForTransactionReceipt({ hash });
  return hash;
}

/** Mint test USDC to the connected account. `amount6` is integer 6dp. */
export async function faucetUsdc(amount6: bigint): Promise<Hex> {
  const { usdc } = await getContracts();
  const net = await getActiveNetworkConfig();
  return writeAndWait(
    (account) =>
      walletClient(net.chain).writeContract({
        address: usdc,
        abi: USDC_FAUCET_ABI,
        functionName: "faucet",
        args: [amount6],
        account,
        chain: net.chain,
      }),
    net.chain,
  );
}

/** Approve the marketplace to spend `amount6` USDC (integer 6dp). */
export async function approveUsdc(amount6: bigint): Promise<Hex> {
  const { usdc, marketplace } = await getContracts();
  const net = await getActiveNetworkConfig();
  return writeAndWait(
    (account) =>
      walletClient(net.chain).writeContract({
        address: usdc,
        abi: ERC20_APPROVE_ABI,
        functionName: "approve",
        args: [marketplace, amount6],
        account,
        chain: net.chain,
      }),
    net.chain,
  );
}

/** Direct (approve-path) purchase: `Marketplace.buy(listingId, recipient)`. */
export async function buyDirect(
  listingId: bigint,
  recipient: Address,
): Promise<Hex> {
  const { marketplace } = await getContracts();
  const net = await getActiveNetworkConfig();
  return writeAndWait(
    (account) =>
      walletClient(net.chain).writeContract({
        address: marketplace,
        abi: MARKETPLACE_BUY_ABI,
        functionName: "buy",
        args: [listingId, recipient],
        account,
        chain: net.chain,
      }),
    net.chain,
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
  /** Chain id + USDC EIP-712 domain for the SIGNING network. Supplied by the
   *  caller from the runtime-resolved {@link getActiveNetworkConfig} so the
   *  signature matches the backend's chain (never a build-time guess). */
  chainId: number;
  usdcDomain: { name: string; version: string };
}) {
  return {
    domain: {
      name: params.usdcDomain.name,
      version: params.usdcDomain.version,
      chainId: params.chainId,
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
  // Resolve the SIGNING network from the backend (strict: throws on a status
  // outage rather than signing with a guessed USDC domain that would revert).
  const net = await getActiveNetworkConfig();
  const message: TransferAuthMessage = {
    from,
    to: marketplace,
    value: valueUsdc6,
    validAfter: 0n,
    validBefore: BigInt(Math.floor(Date.now() / 1000) + validSecs),
    nonce: randomNonce(),
  };
  const typedData = buildTransferAuthTypedData({
    ...message,
    usdc,
    chainId: net.chain.id,
    usdcDomain: net.usdcDomain,
  });
  const signature = await walletClient(net.chain).signTypedData({
    account: from,
    ...typedData,
  });
  return relayBodyFromSignature(message, signature);
}
