// Typed purchase-flow errors. Kept in their own dependency-free module so
// tests can `vi.mock` the chain lib without losing the error classes (an
// `instanceof` check against a mocked class would always be false).

/** Format an integer 6dp USDC amount for operator-facing copy. */
export function formatUsdc6(amount6: bigint): string {
  const s = (Number(amount6) / 1e6).toFixed(2);
  return s.endsWith(".00") ? s.slice(0, -3) : s;
}

/** Thrown when a purchase is attempted without a connected wallet. */
export class WalletRequiredError extends Error {
  constructor() {
    super("Connect a wallet to buy — no wallet connected.");
    this.name = "WalletRequiredError";
  }
}

/**
 * Thrown when the buyer's USDC balance can't cover the listing price.
 * Carries the needed amount so the UI can offer the testnet faucet.
 */
export class InsufficientUsdcError extends Error {
  readonly neededUsdc6: bigint;
  readonly balanceUsdc6: bigint;

  constructor(neededUsdc6: bigint, balanceUsdc6: bigint) {
    super(
      `Insufficient USDC: need ${formatUsdc6(neededUsdc6)} USDC, have ${formatUsdc6(balanceUsdc6)} USDC.`,
    );
    this.name = "InsufficientUsdcError";
    this.neededUsdc6 = neededUsdc6;
    this.balanceUsdc6 = balanceUsdc6;
  }
}
