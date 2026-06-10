import { useQuery } from "@tanstack/react-query";
import { Card } from "@/components/primitives/Card";
import { getIdentity, settingsKeys } from "@/api/settings";
import type { IdentityReport } from "@/api/types.gen";

// ── helpers ──────────────────────────────────────────────────────────────────

function truncate(s: string, head = 6, tail = 4): string {
  if (s.length <= head + tail + 3) return s;
  return `${s.slice(0, head)}…${s.slice(-tail)}`;
}

function MantlescanLink({
  href,
  label,
}: {
  href: string;
  label: string;
}) {
  return (
    <a
      href={href}
      target="_blank"
      rel="noopener noreferrer"
      className="inline-flex items-center gap-1 font-mono text-[12px] text-text hover:text-gold transition-colors"
    >
      <span>{label}</span>
      <span aria-hidden className="text-[10px] text-text-3">
        ↗
      </span>
    </a>
  );
}

// ── configured strip ─────────────────────────────────────────────────────────

function IdentityStrip({ report }: { report: IdentityReport }) {
  const base = report.mantlescan_base_url;
  const hasRegistry = Boolean(report.identity_registry);
  const hasToken = report.agent_token_id !== null && report.agent_token_id !== undefined;
  const hasAttestation = Boolean(report.last_attestation_tx);

  return (
    <Card className="p-5">
      <div className="mb-4">
        <h3 className="m-0 font-sans font-semibold text-[18px] tracking-tight">
          On-Chain Identity
        </h3>
        <p className="m-0 mt-1 text-text-3 text-[12px] leading-snug max-w-2xl">
          Platform agent NFT on Mantle Sepolia. Read-only — minting and
          attestation flows are handled by the chain-attest subsystem.
        </p>
      </div>

      <div className="flex flex-wrap gap-x-6 gap-y-3">
        {/* Token ID */}
        <div>
          <p className="m-0 text-[11px] font-medium text-text-3 uppercase tracking-[0.08em] mb-1">
            Token ID
          </p>
          {hasRegistry && hasToken ? (
            <MantlescanLink
              href={`${base}/token/${report.identity_registry}?a=${report.agent_token_id}`}
              label={String(report.agent_token_id)}
            />
          ) : (
            <span className="font-mono text-[12px] text-text">
              {hasToken ? String(report.agent_token_id) : "—"}
            </span>
          )}
        </div>

        {/* Registry address */}
        <div>
          <p className="m-0 text-[11px] font-medium text-text-3 uppercase tracking-[0.08em] mb-1">
            Registry
          </p>
          {hasRegistry ? (
            <MantlescanLink
              href={`${base}/address/${report.identity_registry}`}
              label={truncate(report.identity_registry!, 6, 4)}
            />
          ) : (
            <span className="font-mono text-[12px] text-text-3">—</span>
          )}
        </div>

        {/* Last attestation */}
        <div>
          <p className="m-0 text-[11px] font-medium text-text-3 uppercase tracking-[0.08em] mb-1">
            Last attestation
          </p>
          {hasAttestation ? (
            <MantlescanLink
              href={`${base}/tx/${report.last_attestation_tx}`}
              label={truncate(report.last_attestation_tx!, 8, 6)}
            />
          ) : (
            <span className="font-mono text-[12px] text-text-3">none yet</span>
          )}
        </div>

        {/* Chain */}
        <div>
          <p className="m-0 text-[11px] font-medium text-text-3 uppercase tracking-[0.08em] mb-1">
            Chain
          </p>
          <span className="font-mono text-[12px] text-text">
            Mantle Sepolia
          </span>
        </div>
      </div>
    </Card>
  );
}

// ── not-configured stub ───────────────────────────────────────────────────────

function IdentityNotConfigured() {
  return (
    <Card className="p-5">
      <div className="flex items-start gap-3">
        <div>
          <h3 className="m-0 font-sans font-semibold text-[18px] tracking-tight text-text-2">
            On-Chain Identity{" "}
            <span className="font-normal text-[14px] text-text-3">
              — not configured
            </span>
          </h3>
          <p className="m-0 mt-1 text-text-3 text-[12px] leading-snug max-w-2xl">
            Set{" "}
            <code className="font-mono text-text-2 text-[11px]">
              XVN_IDENTITY_REGISTRY
            </code>{" "}
            (and optionally{" "}
            <code className="font-mono text-text-2 text-[11px]">
              XVN_PLATFORM_AGENT_TOKEN_ID
            </code>
            ) to enable the on-chain identity strip.
          </p>
        </div>
      </div>
    </Card>
  );
}

// ── route ─────────────────────────────────────────────────────────────────────

export function SettingsIdentityRoute() {
  const q = useQuery({
    queryKey: settingsKeys.identity(),
    queryFn: getIdentity,
  });

  if (q.isPending) {
    return (
      <div className="space-y-5">
        <Card className="p-5 animate-pulse">
          <div className="h-4 w-48 bg-surface-elev rounded mb-3" />
          <div className="h-4 w-80 bg-surface-elev rounded" />
        </Card>
      </div>
    );
  }

  if (q.isError || !q.data) {
    return (
      <div className="space-y-5">
        <Card className="p-5">
          <p className="m-0 text-danger text-[13px] font-mono">
            Failed to load identity data.
          </p>
          <button
            type="button"
            onClick={() => q.refetch()}
            className="mt-3 px-3 py-1.5 rounded text-[12px] border border-border text-text hover:border-text-3"
          >
            Retry
          </button>
        </Card>
      </div>
    );
  }

  const isConfigured = Boolean(q.data.identity_registry);

  return (
    <div className="space-y-5">
      {isConfigured ? (
        <IdentityStrip report={q.data} />
      ) : (
        <IdentityNotConfigured />
      )}
    </div>
  );
}
