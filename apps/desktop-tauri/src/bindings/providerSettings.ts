// Hand-authored bindings mirroring `rust/src/providers/settings_descriptor.rs`.
// The discriminator is `kind`, matching serde's `tag = "kind"`.

export interface PickerOption {
  value: string;
  label: string;
}

export interface SettingsAction {
  id: string;
  label: string;
  destructive: boolean;
}

export type SettingsDescriptor =
  | {
      kind: "toggle";
      key: string;
      title: string;
      subtitle: string | null;
      default: boolean;
    }
  | {
      kind: "field";
      key: string;
      title: string;
      subtitle: string | null;
      placeholder: string | null;
      secret: boolean;
    }
  | {
      kind: "picker";
      key: string;
      title: string;
      subtitle: string | null;
      options: PickerOption[];
      default: string;
    }
  | {
      kind: "actions_row";
      title: string;
      actions: SettingsAction[];
    }
  | {
      kind: "token_accounts";
      title: string;
      subtitle: string | null;
      provider_id: string;
    };

export interface ProviderSettingsContribution {
  provider_id: string;
  section_title: string;
  rows: SettingsDescriptor[];
}

export interface ProviderSettingsSnapshot {
  sections: ProviderSettingsContribution[];
}
