import type { ProviderSnapshot } from "./snapshot";
import { StatusPill } from "./StatusPill";

// Header block — provider name with brand swatch on the first line,
// plan pill right-aligned; email/subtitle on the second line in
// tertiary color. Status pill renders only when non-operational
// (otherwise the operational state is implied and the pill would be
// visual noise).

interface Props {
  snapshot: ProviderSnapshot;
}

function middleTruncate(value: string, maxChars: number): string {
  if (value.length <= maxChars) return value;
  const keep = Math.floor((maxChars - 1) / 2);
  return `${value.slice(0, keep)}…${value.slice(value.length - keep)}`;
}

export function CardHeader({ snapshot }: Props) {
  const showStatus =
    snapshot.status != null && snapshot.status.severity !== "operational";
  const isErrorSubtitle =
    snapshot.subtitle != null &&
    /error|failed|unauthor/i.test(snapshot.subtitle);

  return (
    <header className="card-header">
      <div className="card-header__row">
        <span
          className="card-header__swatch"
          aria-hidden="true"
          style={{ background: snapshot.brandAccent }}
        />
        <span className="card-header__name">{snapshot.displayName}</span>
        {snapshot.plan ? (
          <span className="card-header__plan">{snapshot.plan}</span>
        ) : null}
      </div>
      {snapshot.email ? (
        <div className="card-header__email" title={snapshot.email}>
          {middleTruncate(snapshot.email, 36)}
        </div>
      ) : null}
      {snapshot.subtitle ? (
        <div
          className={
            isErrorSubtitle
              ? "card-header__subtitle card-header__subtitle--error"
              : "card-header__subtitle"
          }
        >
          {snapshot.subtitle}
        </div>
      ) : null}
      {showStatus && snapshot.status ? (
        <div style={{ marginTop: 4 }}>
          <StatusPill
            severity={snapshot.status.severity}
            title={snapshot.status.title}
            statusPageUrl={snapshot.status.statusPageUrl}
          />
        </div>
      ) : null}
    </header>
  );
}
