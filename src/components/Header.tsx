import { useT } from "../i18n/context";
import { RefreshIcon, SettingsIcon } from "./icons";

interface Props {
  canRefresh: boolean;
  refreshing: boolean;
  onRefresh: () => void;
  onOpenSettings: () => void;
}

export function Header({ canRefresh, refreshing, onRefresh, onOpenSettings }: Props) {
  const t = useT();
  const refreshLabel = refreshing ? t("header.refreshing") : t("header.refresh");
  return (
    <header className="header">
      <h1 className="header__title">Canvasist</h1>
      <div className="header__actions">
        {canRefresh && (
          <button
            type="button"
            className={`icon-button${refreshing ? " icon-button--spinning" : ""}`}
            aria-label={refreshLabel}
            title={refreshLabel}
            disabled={refreshing}
            onClick={onRefresh}
          >
            <RefreshIcon />
          </button>
        )}
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
