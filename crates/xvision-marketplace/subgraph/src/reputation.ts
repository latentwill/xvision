import { BigInt } from "@graphprotocol/graph-ts";
import {
  FeedbackPosted,
  FeedbackRevoked,
} from "../generated/ReputationRegistry/ReputationRegistry";
import { Feedback, FeedbackCounter } from "../generated/schema";
import { loadOrCreateAgent } from "./helpers";

// ReputationRegistry.FeedbackPosted → a Feedback row keyed by the on-chain
// append index ("<agentId>-<index>"), assigned from a per-agent FeedbackCounter
// so a later FeedbackRevoked(agentId, index, ...) can address this exact row.
export function handleFeedbackPosted(event: FeedbackPosted): void {
  let agentId = event.params.agentId;
  let agent = loadOrCreateAgent(agentId);

  let counterId = agentId.toString();
  let counter = FeedbackCounter.load(counterId);
  if (counter == null) {
    counter = new FeedbackCounter(counterId);
    counter.count = BigInt.zero();
  }
  let index = counter.count;

  let fb = new Feedback(agentId.toString() + "-" + index.toString());
  fb.agent = agent.id;
  fb.rater = event.params.rater;
  fb.value = event.params.value;
  fb.feedbackHash = event.params.feedbackHash;
  fb.tag1 = event.params.tag1;
  fb.revoked = false;
  fb.blockTimestamp = event.block.timestamp;
  fb.save();

  counter.count = index.plus(BigInt.fromI32(1));
  counter.save();
}

// ReputationRegistry.FeedbackRevoked → §3.7 tombstone. Aggregate recompute on
// the read side excludes revoked entries.
export function handleFeedbackRevoked(event: FeedbackRevoked): void {
  let id =
    event.params.agentId.toString() + "-" + event.params.index.toString();
  let fb = Feedback.load(id);
  if (fb == null) return;
  fb.revoked = true;
  fb.save();
}
