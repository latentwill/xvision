import { Sold } from "../generated/Marketplace/Marketplace";
import { Sale } from "../generated/schema";
import { loadOrCreateListing } from "./helpers";

// Marketplace.Sold → a Sale row. `payerKind`/`purchasePath` are the
// human-vs-agent moat fields (purchasePath: 0 = direct, 1 = x402); payerKind is
// a v1 placeholder mirroring purchasePath — see the schema note, do not derive
// analytics from it.
export function handleSold(event: Sold): void {
  // The listing exists from ListingCreated; agentNftId on Sold lets the helper
  // satisfy the non-null link defensively if ordering ever differs.
  let listing = loadOrCreateListing(event.params.listingId, event.params.agentNftId);
  let id =
    event.transaction.hash.toHexString() + "-" + event.logIndex.toString();
  let sale = new Sale(id);
  sale.listing = listing.id;
  sale.buyer = event.params.buyer;
  sale.priceUSDC = event.params.priceUSDC;
  sale.sellerProceeds = event.params.sellerProceeds;
  sale.protocolProceeds = event.params.protocolProceeds;
  sale.payerKind = event.params.payerKind;
  sale.purchasePath = event.params.purchasePath;
  sale.blockTimestamp = event.block.timestamp;
  sale.save();
}
