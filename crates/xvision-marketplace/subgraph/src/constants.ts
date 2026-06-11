import { Address } from "@graphprotocol/graph-ts";

// Deployed Marketplace UUPS proxy on Mantle Sepolia (config/mantle-sepolia.toml,
// unchanged across the 2026-06-10 setUsdc upgrade — only the impl moved). Used
// solely to snapshot the global `protocolFeeBps` onto a Listing at creation,
// since ListingCreated does not carry the fee. Update this when redeploying to
// a different network/address.
export const MARKETPLACE_ADDRESS = Address.fromString(
  "0x4b9450642f2b3Da248e90b4FEBaA8eCA87E78cE8"
);
