import { useCallback, useEffect, useState } from "react";
import { EmptyState } from "./components/EmptyState";
import { Header } from "./components/Header";
import { SettingsView } from "./components/SettingsView";
import { I18nProvider } from "./i18n/context";
import { api, errorMessage, type LanguagePreference, type SettingsView as Settings } from "./ipc";

type View = "home" | "settings";

export default function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [view, setView] = useState<View>("home");

  useEffect(() => {
    api.getSettings().then(setSettings, (e: unknown) => setLoadError(errorMessage(e)));
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
  if (!settings) return null;

  return (
    <I18nProvider locale={settings.effectiveLocale}>
      <div className="app">
        {view === "home" ? (
          <>
            <Header onOpenSettings={() => setView("settings")} />
            <main className="main">
              <EmptyState />
            </main>
          </>
        ) : (
          <main className="main">
            <SettingsView
              language={settings.language}
              onLanguageChange={changeLanguage}
              onBack={() => setView("home")}
            />
          </main>
        )}
      </div>
    </I18nProvider>
  );
}
