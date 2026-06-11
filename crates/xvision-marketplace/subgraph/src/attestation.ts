import { AttestationPosted } from "../generated/EvalAttestationRegistry/EvalAttestationRegistry";
import { EvalAttestation, Listing } from "../generated/schema";

// EvalAttestationRegistry.AttestationPosted → an EvalAttestation row linked to
// its Listing. The §3.6 attestation engine posts these (value=verdict,
// decimals=0) after each 20-trade window.
export function handleAttestationPosted(event: AttestationPosted): void {
  let listing = Listing.load(event.params.listingId.toString());
  // An attestation for a listing we never indexed is anomalous (on-chain the
  // listing must exist first); skip rather than fabricate a placeholder.
  if (listing == null) return;
  let id =
    event.transaction.hash.toHexString() + "-" + event.logIndex.toString();
  let att = new EvalAttestation(id);
  att.listing = listing.id;
  att.attester = event.params.attester;
  att.evalResultHash = event.params.evalResultHash;
  att.schema = event.params.schema;
  att.postedAt = event.block.timestamp;
  att.save();
}
