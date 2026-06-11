import { Address, BigInt, Bytes } from "@graphprotocol/graph-ts";
import { Agent, Listing } from "../generated/schema";

// Load an Agent by its IdentityRegistry token id, creating a placeholder if the
// AgentRegistered event has not been indexed yet (e.g. a Listing/Feedback that
// references an agent minted in the same block). handleAgentRegistered later
// overwrites owner/manifestCid with the real values. Keeps the non-null links
// on Listing/Feedback/Validation satisfiable regardless of event ordering.
export function loadOrCreateAgent(agentId: BigInt): Agent {
  let id = agentId.toString();
  let agent = Agent.load(id);
  if (agent == null) {
    agent = new Agent(id);
    agent.owner = Address.zero();
    agent.manifestCid = "";
    agent.save();
  }
  return agent;
}

// Load a Listing by id, creating a placeholder linked to `agentId` if missing.
// On-chain a listing is always created (ListingCreated) before it can be sold
// or attested, so this normally just returns the existing row; the create path
// is a defensive guard that keeps Sale.listing / EvalAttestation.listing
// non-null even under unexpected ordering. Real field values are set by
// handleListingCreated.
export function loadOrCreateListing(listingId: BigInt, agentId: BigInt): Listing {
  let id = listingId.toString();
  let listing = Listing.load(id);
  if (listing == null) {
    let agent = loadOrCreateAgent(agentId);
    listing = new Listing(id);
    listing.agent = agent.id;
    listing.seller = Address.zero();
    listing.contentHash = Bytes.fromHexString("0x");
    listing.tier = 0;
    listing.priceUSDC = BigInt.zero();
    listing.protocolFeeBps = 0;
    listing.revoked = false;
    listing.save();
  }
  return listing;
}
