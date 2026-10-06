import { useEffect, useId, useState, type FormEvent } from "react";
import { describeError } from "../errors";
import { useT } from "../i18n/context";
import { api, toAppError, type AppError, type School } from "../ipc";
import { LogoMark } from "./icons";

const SEARCH_DELAY_MS = 300;

interface Props {
  onStartLogin: (origin: string) => Promise<void>;
}

export function ConnectView({ onStartLogin }: Props) {
  const t = useT();
  const [mode, setMode] = useState<"search" | "manual">("search");
  return (
    <section className="connect" aria-labelledby="connect-title">
      <LogoMark size={64} />
      <h2 id="connect-title" className="connect__title">
        {t("connect.title")}
      </h2>
      <p className="connect__body">{t("connect.body")}</p>

      {mode === "search" ? (
        <SchoolSearch onStartLogin={onStartLogin} />
      ) : (
        <ManualAddress onStartLogin={onStartLogin} />
      )}

      <button
        type="button"
        className="link-button"
        onClick={() => setMode(mode === "search" ? "manual" : "search")}
      >
        {mode === "search" ? t("connect.manualLink") : t("connect.backToSearch")}
      </button>
      <p className="connect__privacy">{t("connect.privacy")}</p>
    </section>
  );
}

function SchoolSearch({ onStartLogin }: Props) {
  const t = useT();
  const inputId = useId();
  const [query, setQuery] = useState("");
  // Results remember the query they answer, so stale responses are never shown.
  const [results, setResults] = useState<{ query: string; schools: School[] } | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [starting, setStarting] = useState<string | null>(null);

  const trimmed = query.trim();
  const active = trimmed.length >= 2;
  const current = active && results?.query === trimmed ? results.schools : null;

  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    const timer = setTimeout(() => {
      api.searchSchools(trimmed).then(
        (schools) => {
          if (cancelled) return;
          setResults({ query: trimmed, schools });
          setError(null);
        },
        (e: unknown) => {
          if (!cancelled) setError(toAppError(e));
        },
      );
    }, SEARCH_DELAY_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [active, trimmed]);

  async function choose(school: School) {
    setStarting(school.url);
    setError(null);
    try {
      await onStartLogin(school.url);
    } catch (e) {
      setError(toAppError(e));
    } finally {
      setStarting(null);
    }
  }

  return (
    <div className="connect__form">
      <label htmlFor={inputId} className="field-label">
        {t("connect.searchLabel")}
      </label>
      <input
        id={inputId}
        className="text-input"
        type="search"
        autoComplete="off"
        spellCheck={false}
        placeholder={t("connect.searchPlaceholder")}
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        maxLength={100}
        autoFocus
      />
      {error && (
        <p className="form-error" role="alert">
          {describeError(t, error)}
        </p>
      )}
      {active && !current && !error && <p className="form-hint">{t("connect.searching")}</p>}
      {current && current.length === 0 && <p className="form-hint">{t("connect.noResults")}</p>}
      {current && current.length > 0 && (
        <ul className="school-list">
          {current.map((school) => (
            <li key={`${school.name}|${school.url}`}>
              <button
                type="button"
                className="school-list__item"
                disabled={starting !== null}
                onClick={() => choose(school)}
              >
                <span className="school-list__name">{school.name}</span>
                <span className="school-list__url">{new URL(school.url).host}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function ManualAddress({ onStartLogin }: Props) {
  const t = useT();
  const inputId = useId();
  const [value, setValue] = useState("");
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState<AppError | null>(null);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setChecking(true);
    setError(null);
    try {
      const origin = await api.checkCanvasUrl(value);
      await onStartLogin(origin);
    } catch (err) {
      setError(toAppError(err));
    } finally {
      setChecking(false);
    }
  }

  return (
    <form className="connect__form" onSubmit={submit}>
      <label htmlFor={inputId} className="field-label">
        {t("connect.manualLabel")}
      </label>
      <input
        id={inputId}
        className="text-input"
        type="text"
        inputMode="url"
        autoComplete="url"
        spellCheck={false}
        placeholder={t("connect.manualPlaceholder")}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        maxLength={255}
        autoFocus
      />
      {error && (
        <p className="form-error" role="alert">
          {describeError(t, error)}
        </p>
      )}
      <button type="submit" className="primary-button" disabled={checking || value.trim() === ""}>
        {checking ? t("connect.checking") : t("connect.manualSubmit")}
      </button>
    </form>
  );
}
