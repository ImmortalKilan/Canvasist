import { useCallback, useEffect, useMemo, useState } from "react";
import { AssignmentList } from "./components/AssignmentList";
import { ConnectView } from "./components/ConnectView";
import { Header } from "./components/Header";
import { LoginPending } from "./components/LoginPending";
import { SettingsView } from "./components/SettingsView";
import { I18nProvider, useT } from "./i18n/context";
import {
  api,
  errorMessage,
  events,
  toAppError,
  type AppError,
  type AuthStatus,
  type Snapshot,
  type LanguagePreference,
  type Preferences,
  type SettingsView as Settings,
} from "./ipc";

type View = "home" | "settings";

const BUSY_RETRIES = 60;
const BUSY_RETRY_MS = 2000;

export default function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [auth, setAuth] = useState<AuthStatus | null>(null);
  const [preferences, setPreferences] = useState<Preferences | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([api.getSettings(), api.getAuthStatus(), api.getPreferences()]).then(
      ([s, a, p]) => {
        setSettings(s);
        setAuth(a);
        setPreferences(p);
      },
      (e: unknown) => setLoadError(errorMessage(e)),
    );
  }, []);

  useEffect(() => {
    if (settings) document.documentElement.lang = settings.effectiveLocale;
  }, [settings]);

  const changeLanguage = useCallback(async (language: LanguagePreference) => {
    setSettings(await api.setLanguage(language));
  }, []);

  if (loadError) {
    // Settings are needed to pick a language, so this one message is bilingual.
    return (
      <p className="fatal" role="alert">
        Canvasist failed to start / 启动失败: {loadError}
      </p>
    );
  }
  // Render nothing until the language is known, to avoid a flash of the wrong language.
  if (!settings || !auth || !preferences) return null;

  return (
    <I18nProvider locale={settings.effectiveLocale}>
      <Shell
        settings={settings}
        auth={auth}
        preferences={preferences}
        onAuthChange={setAuth}
        onPreferencesChange={setPreferences}
        onLanguageChange={changeLanguage}
      />
    </I18nProvider>
  );
}

interface ShellProps {
  settings: Settings;
  auth: AuthStatus;
  preferences: Preferences;
  onAuthChange: (auth: AuthStatus) => void;
  onPreferencesChange: (preferences: Preferences) => void;
  onLanguageChange: (language: LanguagePreference) => Promise<void>;
}

function Shell({
  settings,
  auth,
  preferences,
  onAuthChange,
  onPreferencesChange,
  onLanguageChange,
}: ShellProps) {
  const t = useT();
  const [view, setView] = useState<View>("home");
  const [loginPending, setLoginPending] = useState(false);
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [refreshError, setRefreshError] = useState<AppError | null>(null);

  // `waitIfBusy`: a refresh that is already running started before something
  // changed (such as a Gradescope sign-in), so wait for it and run again.
  const refresh = useCallback(async (waitIfBusy = false) => {
    setRefreshing(true);
    setRefreshError(null);
    try {
      for (let attempt = 1; ; attempt++) {
        try {
          setSnapshot(await api.refresh());
          return;
        } catch (e) {
          const error = toAppError(e);
          if (error.kind === "busy" && waitIfBusy && attempt < BUSY_RETRIES) {
            await new Promise((resolve) => setTimeout(resolve, BUSY_RETRY_MS));
            continue;
          }
          // "busy" means a refresh is already running; its result will arrive on its own.
          // "sessionExpired" is shown by the banner via the auth-changed event.
          if (error.kind !== "busy" && error.kind !== "sessionExpired") setRefreshError(error);
          return;
        }
      }
    } finally {
      setRefreshing(false);
    }
  }, []);

  // Show the cached snapshot immediately (also while the session is expired),
  // then fetch fresh data whenever the session is valid.
  const authState = auth.state;
  useEffect(() => {
    if (authState === "signedOut") return;
    let cancelled = false;
    api
      .getSnapshot()
      .then((cached) => {
        if (!cancelled && cached) setSnapshot(cached);
      })
      .finally(() => {
        if (!cancelled && authState === "signedIn") void refresh();
      });
    return () => {
      cancelled = true;
    };
  }, [authState, refresh]);

  useEffect(() => {
    const subscriptions = [
      events.onAuthChanged((status) => {
        setLoginPending(false);
        if (status.state === "signedOut") setSnapshot(null);
        onAuthChange(status);
      }),
      events.onLoginCancelled(() => setLoginPending(false)),
      // Hourly and wake-up refreshes run in the background and arrive here.
      events.onSnapshotUpdated(setSnapshot),
      events.onGradescopeLoginCompleted(() => void refresh(true)),
    ];
    return () => {
      for (const s of subscriptions) void s.then((unlisten) => unlisten());
    };
  }, [onAuthChange, refresh]);

  // Hidden courses (settings) are filtered out of the list here.
  const visibleSnapshot = useMemo(() => {
    if (!snapshot || preferences.hiddenCourses.length === 0) return snapshot;
    const hidden = new Set(preferences.hiddenCourses);
    return {
      ...snapshot,
      courses: snapshot.courses.filter((c) => !hidden.has(c.id)),
      assignments: snapshot.assignments.filter((a) => !hidden.has(a.courseId)),
    };
  }, [snapshot, preferences.hiddenCourses]);

  const startLogin = useCallback(async (origin: string) => {
    await api.startCanvasLogin(origin);
    setLoginPending(true);
  }, []);

  const startGradescopeLogin = useCallback(async () => {
    try {
      await api.startGradescopeLogin();
    } catch (e) {
      setRefreshError(toAppError(e));
    }
  }, []);

  const signOut = useCallback(async () => {
    await api.signOut();
    setView("home");
  }, []);

  if (view === "settings") {
    return (
      <div className="app">
        <main className="main">
          <SettingsView
            language={settings.language}
            auth={auth}
            preferences={preferences}
            courses={snapshot?.courses ?? []}
            onPreferencesChange={(next) => {
              const refetch = next.showUnsubmittable !== preferences.showUnsubmittable;
              onPreferencesChange(next);
              // That setting changes which Canvas items are fetched.
              if (refetch) void refresh();
            }}
            onLanguageChange={onLanguageChange}
            onSignOut={signOut}
            onBack={() => setView("home")}
          />
        </main>
      </div>
    );
  }

  return (
    <div className="app">
      <Header
        canRefresh={auth.state !== "signedOut" && !loginPending}
        refreshing={refreshing}
        onRefresh={() => void refresh()}
        onOpenSettings={() => setView("settings")}
      />
      <main className="main">
        {loginPending ? (
          <LoginPending
            onCancel={() => {
              void api.cancelCanvasLogin();
              setLoginPending(false);
            }}
          />
        ) : auth.state === "signedOut" ? (
          <ConnectView onStartLogin={startLogin} />
        ) : (
          <>
            {auth.state === "expired" && (
              <div className="banner" role="status">
                <span>{t("banner.expired")}</span>
                <button
                  type="button"
                  className="banner__action"
                  onClick={() => void startLogin(auth.origin)}
                >
                  {t("banner.relogin")}
                </button>
              </div>
            )}
            {auth.state === "signedIn" && snapshot?.gradescope === "needsCanvasLogin" && (
              <div className="banner" role="status">
                <span>{t("banner.gradescopeLogin")}</span>
                <button
                  type="button"
                  className="banner__action"
                  onClick={() => void startLogin(auth.origin)}
                >
                  {t("banner.relogin")}
                </button>
              </div>
            )}
            {auth.state === "signedIn" && snapshot?.gradescope === "needsGradescopeLogin" && (
              <div className="banner" role="status">
                <span>{t("banner.gradescopeDirect")}</span>
                <button
                  type="button"
                  className="banner__action"
                  onClick={() => void startGradescopeLogin()}
                >
                  {t("banner.gradescopeDirectAction")}
                </button>
              </div>
            )}
            {snapshot?.gradescope === "unavailable" && (
              <p className="form-hint" role="status">
                {t("banner.gradescopeUnavailable")}
              </p>
            )}
            <AssignmentList
              snapshot={visibleSnapshot}
              locale={settings.effectiveLocale}
              error={refreshError}
            />
          </>
        )}
      </main>
    </div>
  );
}
