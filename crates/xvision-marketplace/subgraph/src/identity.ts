import { AgentRegistered } from "../generated/IdentityRegistry/IdentityRegistry";
import { loadOrCreateAgent } from "./helpers";

// IdentityRegistry.AgentRegistered → the canonical Agent row (one per listed
// strategy under AM3). May upgrade a placeholder created by an earlier
// Listing/Feedback in the same block.
export function handleAgentRegistered(event: AgentRegistered): void {
  let agent = loadOrCreateAgent(event.params.tokenId);
  agent.owner = event.params.owner;
  agent.manifestCid = event.params.agentURI;
  agent.save();
}
