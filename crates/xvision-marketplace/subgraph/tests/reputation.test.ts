import {
  assert,
  describe,
  test,
  clearStore,
  afterEach,
  newMockEvent,
} from "matchstick-as/assembly/index";
import { Address, BigInt, Bytes, ethereum } from "@graphprotocol/graph-ts";
import {
  FeedbackPosted,
  FeedbackRevoked,
} from "../generated/ReputationRegistry/ReputationRegistry";
import {
  handleFeedbackPosted,
  handleFeedbackRevoked,
} from "../src/reputation";

const AGENT = "7";
const RATER = "0x00000000000000000000000000000000000000aa";
const HASH = "0x1234000000000000000000000000000000000000000000000000000000000000";

function newFeedbackPosted(
  agentId: string,
  value: i32,
  tag1: string
): FeedbackPosted {
  let e = changetype<FeedbackPosted>(newMockEvent());
  e.parameters = new Array();
  e.parameters.push(
    new ethereum.EventParam(
      "agentId",
      ethereum.Value.fromUnsignedBigInt(BigInt.fromString(agentId))
    )
  );
  e.parameters.push(
    new ethereum.EventParam(
      "rater",
      ethereum.Value.fromAddress(Address.fromString(RATER))
    )
  );
  e.parameters.push(
    new ethereum.EventParam(
      "value",
      ethereum.Value.fromSignedBigInt(BigInt.fromI32(value))
    )
  );
  e.parameters.push(
    new ethereum.EventParam(
      "feedbackHash",
      ethereum.Value.fromBytes(Bytes.fromHexString(HASH))
    )
  );
  e.parameters.push(
    new ethereum.EventParam("tag1", ethereum.Value.fromString(tag1))
  );
  return e;
}

function newFeedbackRevoked(agentId: string, index: i32): FeedbackRevoked {
  let e = changetype<FeedbackRevoked>(newMockEvent());
  e.parameters = new Array();
  e.parameters.push(
    new ethereum.EventParam(
      "agentId",
      ethereum.Value.fromUnsignedBigInt(BigInt.fromString(agentId))
    )
  );
  e.parameters.push(
    new ethereum.EventParam(
      "index",
      ethereum.Value.fromUnsignedBigInt(BigInt.fromI32(index))
    )
  );
  e.parameters.push(
    new ethereum.EventParam(
      "revoker",
      ethereum.Value.fromAddress(Address.fromString(RATER))
    )
  );
  return e;
}

describe("ReputationRegistry mappings", () => {
  afterEach(() => {
    clearStore();
  });

  test("feedback rows are keyed by per-agent append index", () => {
    handleFeedbackPosted(newFeedbackPosted(AGENT, 100, "tradingYield"));
    handleFeedbackPosted(newFeedbackPosted(AGENT, 50, "tradingYield"));

    // Two distinct rows, indexed 0 and 1, both linked to the agent.
    assert.entityCount("Feedback", 2);
    assert.fieldEquals("Feedback", AGENT + "-0", "value", "100");
    assert.fieldEquals("Feedback", AGENT + "-1", "value", "50");
    assert.fieldEquals("Feedback", AGENT + "-0", "agent", AGENT);
    assert.fieldEquals("Feedback", AGENT + "-0", "revoked", "false");
    // Counter advanced to 2 (next index).
    assert.fieldEquals("FeedbackCounter", AGENT, "count", "2");
  });

  test("FeedbackRevoked tombstones the entry at that index", () => {
    handleFeedbackPosted(newFeedbackPosted(AGENT, 100, "tradingYield"));
    handleFeedbackPosted(newFeedbackPosted(AGENT, 50, "tradingYield"));

    handleFeedbackRevoked(newFeedbackRevoked(AGENT, 1));

    assert.fieldEquals("Feedback", AGENT + "-1", "revoked", "true");
    // The other entry is untouched.
    assert.fieldEquals("Feedback", AGENT + "-0", "revoked", "false");
  });

  test("revoking an unknown index is a no-op (no entity created)", () => {
    handleFeedbackRevoked(newFeedbackRevoked(AGENT, 0));
    assert.entityCount("Feedback", 0);
  });
});
