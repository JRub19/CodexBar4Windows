// Hand authored bindings mirroring `rust/src/providers/descriptor.rs` and
// `rust/src/core/events.rs`.
//
// Phase 1 ships an empty registry, so `provider_descriptors` returns `[]`.
// Phase 4 begins populating these shapes with real data.

export type FetchStrategy = "OAuth" | "Web" | "CLI" | "ApiKey";

export interface ProviderMetadataDto {
  display_name: string;
  homepage: string;
  dashboard_url: string | null;
}

export interface ProviderBrandingDto {
  accent_hex: string;
  icon_id: string;
}

export interface ProviderCLIConfigDto {
  binary_name: string;
  default_args: string[];
}

export interface ProviderFetchPlanDto {
  strategies: FetchStrategy[];
}

export interface ProviderDescriptorDto {
  id: string;
  metadata: ProviderMetadataDto;
  branding: ProviderBrandingDto;
  cli: ProviderCLIConfigDto | null;
  fetch_plan: ProviderFetchPlanDto;
}

export interface UsageEventPayload {
  provider: string;
  menu_rev: number;
  icon_rev: number;
}

export interface StatusEventPayload {
  provider: string;
  severity: "operational" | "degraded" | "partial_outage" | "major_outage" | "investigating";
  title: string | null;
}
