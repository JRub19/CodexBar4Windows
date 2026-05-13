import { ProviderSwitcherButtons } from "./ProviderSwitcherButtons";

// The header region holds only the provider switcher (when >=2
// providers are enabled). The original mac app has no top-of-popup
// title bar or settings cog — those live in the footer action rows
// alongside Refresh / About / Quit. Removing the invented chrome
// brings the popup back to the menu-style stacked layout.

export function PopupHeader() {
  return (
    <header className="popup-header">
      <ProviderSwitcherButtons />
    </header>
  );
}
