import { useT } from "../i18n/context";
import { LogoMark } from "./icons";

export function LoginPending({ onCancel }: { onCancel: () => void }) {
  const t = useT();
  return (
    <section className="connect" aria-labelledby="login-title" aria-live="polite">
      <div className="pulse">
        <LogoMark size={64} />
      </div>
      <h2 id="login-title" className="connect__title">
        {t("login.title")}
      </h2>
      <p className="connect__body">{t("login.body")}</p>
      <button type="button" className="secondary-button" onClick={onCancel}>
        {t("login.cancel")}
      </button>
    </section>
  );
}
