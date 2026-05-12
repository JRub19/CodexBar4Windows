import type { ProviderDescriptorDto } from "../../bindings";

// Phase 3 D4 placeholder. D5 fills in header, metrics, status pill.
// Keeping the stub here so CardStack compiles in its own commit.

interface Props {
  descriptor: ProviderDescriptorDto;
}

export function ProviderCard({ descriptor }: Props) {
  return (
    <article className="provider-card">
      <header className="provider-card__header">
        {descriptor.metadata.display_name}
      </header>
      <p className="provider-card__placeholder">
        Live usage metrics arrive in phase 4 when this provider's fetch
        strategies are wired up.
      </p>
    </article>
  );
}
