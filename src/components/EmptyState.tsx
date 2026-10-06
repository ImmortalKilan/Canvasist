import { useT } from "../i18n/context";
import { LogoMark } from "./icons";

export function EmptyState() {
  const t = useT();
  return (
    <section className="empty" aria-labelledby="empty-title">
      <LogoMark size={72} />
      <h2 id="empty-title" className="empty__title">
        {t("empty.title")}
      </h2>
      <p className="empty__body">{t("empty.body")}</p>
      <button type="button" className="primary-button" disabled aria-describedby="empty-soon">
        {t("empty.connect")}
      </button>
      <p id="empty-soon" className="empty__note">
        {t("empty.comingSoon")}
      </p>
    </section>
  );
}
