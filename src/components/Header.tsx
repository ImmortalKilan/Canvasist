import { useT } from "../i18n/context";
import { RefreshIcon, SettingsIcon } from "./icons";

export function Header({ onOpenSettings }: { onOpenSettings: () => void }) {
  const t = useT();
  return (
    <header className="header">
      <h1 className="header__title">Canvasist</h1>
      <div className="header__actions">
        {/* Refreshing becomes available once Canvas is connected (M2). */}
        <button
          type="button"
          className="icon-button"
          aria-label={t("header.refresh")}
          title={t("header.refreshUnavailable")}
          disabled
        >
          <RefreshIcon />
        </button>
        <button
          type="button"
          className="icon-button"
          aria-label={t("header.settings")}
          title={t("header.settings")}
          onClick={onOpenSettings}
        >
          <SettingsIcon />
        </button>
      </div>
    </header>
  );
}
