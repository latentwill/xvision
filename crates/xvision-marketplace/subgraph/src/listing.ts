import {
  ListingCreated,
  ListingUpdated,
  ListingRevoked,
} from "../generated/ListingRegistry/ListingRegistry";
import { Marketplace } from "../generated/ListingRegistry/Marketplace";
import { Listing } from "../generated/schema";
import { loadOrCreateAgent } from "./helpers";
import { MARKETPLACE_ADDRESS } from "./constants";

// ListingRegistry.ListingCreated → a Listing row (1:1 with its Agent under AM3).
export function handleListingCreated(event: ListingCreated): void {
  let agent = loadOrCreateAgent(event.params.agentNftId);
  let listing = new Listing(event.params.listingId.toString());
  listing.agent = agent.id;
  listing.seller = event.params.seller;
  listing.contentHash = event.params.contentHash;
  listing.tier = event.params.tier;
  listing.priceUSDC = event.params.priceUSDC;
  // Snapshot the global protocol fee at creation time. ListingCreated does not
  // carry it, so read Marketplace.protocolFeeBps() via eth_call with a graceful
  // fallback — a reverting/absent call must not drop an otherwise valid listing.
  let mp = Marketplace.bind(MARKETPLACE_ADDRESS);
  let fee = mp.try_protocolFeeBps();
  listing.protocolFeeBps = fee.reverted ? 0 : fee.value;
  listing.revoked = false;
  listing.save();
}

// ListingRegistry.ListingUpdated → repoint the content hash. (contentURI is not
// stored on-chain in the schema; the manifest CID lives on the Agent.)
export function handleListingUpdated(event: ListingUpdated): void {
  let listing = Listing.load(event.params.listingId.toString());
  if (listing == null) return;
  listing.contentHash = event.params.contentHash;
  listing.save();
}

// ListingRegistry.ListingRevoked → tombstone (the listing stays queryable so
// historical sales/attestations keep their non-null link).
export function handleListingRevoked(event: ListingRevoked): void {
  let listing = Listing.load(event.params.listingId.toString());
  if (listing == null) return;
  listing.revoked = true;
  listing.save();
}
