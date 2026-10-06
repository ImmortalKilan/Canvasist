import { useEffect, useState } from "react";
import { useT } from "../i18n/context";
import { api, errorMessage, type LanguagePreference } from "../ipc";
import { BackIcon } from "./icons";

interface Props {
  language: LanguagePreference;
  onLanguageChange: (language: LanguagePreference) => Promise<void>;
  onBack: () => void;
}

// Native names are shown in every locale so users can always find their language.
const LANGUAGE_OPTIONS: readonly { value: LanguagePreference; nativeLabel?: string }[] = [
  { value: "system" },
  { value: "en", nativeLabel: "English" },
  { value: "zh-CN", nativeLabel: "简体中文" },
];

export function SettingsView({ language, onLanguageChange, onBack }: Props) {
  const t = useT();
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [version, setVersion] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let cancelled = false;
    Promise.all([api.getAutostart(), api.getAppInfo()])
      .then(([enabled, info]) => {
        if (cancelled) return;
        setAutostart(enabled);
        setVersion(info.version);
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(errorMessage(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function run(action: () => Promise<void>) {
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="settings" aria-labelledby="settings-title">
      <div className="settings__bar">
        <button
          type="button"
          className="icon-button"
          aria-label={t("settings.back")}
          title={t("settings.back")}
          onClick={onBack}
        >
          <BackIcon />
        </button>
        <h2 id="settings-title" className="settings__title">
          {t("settings.title")}
        </h2>
      </div>

      {error && (
        <p className="settings__error" role="alert">
          {t("error.generic", { message: error })}
        </p>
      )}

      <fieldset className="settings__group">
        <legend className="settings__label">{t("settings.language")}</legend>
        <div className="segmented">
          {LANGUAGE_OPTIONS.map((option) => (
            <label key={option.value} className="segmented__option">
              <input
                type="radio"
                name="language"
                value={option.value}
                checked={language === option.value}
                disabled={busy}
                onChange={() => run(() => onLanguageChange(option.value))}
              />
              <span>{option.nativeLabel ?? t("settings.language.system")}</span>
            </label>
          ))}
        </div>
      </fieldset>

      <div className="settings__group settings__row">
        <div>
          <p className="settings__label" id="autostart-label">
            {t("settings.autostart")}
          </p>
          <p className="settings__hint">{t("settings.autostart.hint")}</p>
        </div>
        <button
          type="button"
          role="switch"
          className="switch"
          aria-checked={autostart ?? false}
          aria-labelledby="autostart-label"
          disabled={busy || autostart === null}
          onClick={() =>
            run(async () => {
              setAutostart(await api.setAutostart(!autostart));
            })
          }
        >
          <span className="switch__thumb" />
        </button>
      </div>

      <div className="settings__group">
        <p className="settings__label">{t("settings.about")}</p>
        <p className="settings__hint">
          Canvasist{version && ` · ${t("settings.version", { version })}`} · MIT License
        </p>
      </div>
    </section>
  );
}
