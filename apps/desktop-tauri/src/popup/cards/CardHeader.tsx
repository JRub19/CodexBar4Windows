import type { ProviderSnapshot } from "./snapshot";
import { StatusPill } from "./StatusPill";

// Phase 3 D5: top region of the card. Stacks provider name, optional
// email (middle truncated), optional subtitle, and the status pill
// (when not operational). Plan name renders as a 11 px secondary pill
// to match spec 15 section 3.

interface Props {
  snapshot: ProviderSnapshot;
}

function middleTruncate(value: string, max: number) {
  if (value.length <= max) return value;
  const head = Math.ceil(max / 2) - 1;
  const tail = Math.floor(max / 2) - 1;
  return `${value.slice(0, head)}…${value.slice(-tail)}`;
}

export function CardHeader({ snapshot }: Props) {
  return (
    <header className="card-header">
      <div className="card-header__row">
        <span className="card-header__name">{snapshot.displayName}</span>
        {snapshot.plan ? (
          <span className="card-header__plan">{snapshot.plan}</span>
        ) : null}
      </div>
      {snapshot.email ? (
        <span className="card-header__email" title={snapshot.email}>
          {middleTruncate(snapshot.email, 32)}
        </span>
      ) : null}
      {snapshot.subtitle ? (
        <span className="card-header__subtitle">{snapshot.subtitle}</span>
      ) : null}
      {snapshot.status ? (
        <StatusPill
          severity={snapshot.status.severity}
          title={snapshot.status.title}
          statusPageUrl={null}
        />
      ) : null}
    </header>
  );
}
