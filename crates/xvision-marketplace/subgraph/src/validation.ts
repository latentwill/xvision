import { ValidationPosted } from "../generated/ValidationRegistry/ValidationRegistry";
import { Validation } from "../generated/schema";
import { loadOrCreateAgent } from "./helpers";

// ValidationRegistry.ValidationPosted → a Validation row. Permissionless;
// trust-weighting (in-house vs external attester) is a read-side concern.
export function handleValidationPosted(event: ValidationPosted): void {
  let agent = loadOrCreateAgent(event.params.agentId);
  let id =
    event.transaction.hash.toHexString() + "-" + event.logIndex.toString();
  let v = new Validation(id);
  v.agent = agent.id;
  v.validator = event.params.validator;
  v.resultHash = event.params.resultHash;
  v.tag = event.params.tag;
  v.blockTimestamp = event.block.timestamp;
  v.save();
}
