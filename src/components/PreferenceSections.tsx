import { useId } from "react";
import type { TranslateFn } from "../i18n/context";
import { useT } from "../i18n/context";
import type { Course } from "../ipc";

/** Reminder choices offered in the "Add a reminder" menu, in minutes. */
const PRESETS = [15, 30, 60, 120, 180, 360, 720, 1440, 2880, 4320, 10080] as const;
const MAX_REMINDERS = 8;

export function offsetLabel(t: TranslateFn, minutes: number): string {
  if (minutes % 1440 === 0) {
    const n = minutes / 1440;
    return n === 1 ? t("settings.reminders.day") : t("settings.reminders.days", { n });
  }
  if (minutes % 60 === 0) {
    const n = minutes / 60;
    return n === 1 ? t("settings.reminders.hour") : t("settings.reminders.hours", { n });
  }
  return t("settings.reminders.minutes", { n: minutes });
}

interface RemindersProps {
  offsets: readonly number[];
  disabled: boolean;
  onChange: (minutes: number[]) => void;
}

export function ReminderSettings({ offsets, disabled, onChange }: RemindersProps) {
  const t = useT();
  const selectId = useId();
  const available = PRESETS.filter((m) => !offsets.includes(m));
  return (
    <div className="settings__group">
      <p className="settings__label">{t("settings.reminders")}</p>
      <p className="settings__hint">{t("settings.reminders.hint")}</p>
      <ul className="chips">
        {offsets.length === 0 && <li className="settings__hint">{t("settings.reminders.none")}</li>}
        {offsets.map((m) => {
          const label = offsetLabel(t, m);
          return (
            <li key={m} className="chip">
              {label}
              <button
                type="button"
                className="chip__remove"
                aria-label={t("settings.reminders.remove", { offset: label })}
                title={t("settings.reminders.remove", { offset: label })}
                disabled={disabled}
                onClick={() => onChange(offsets.filter((x) => x !== m))}
              >
                ×
              </button>
            </li>
          );
        })}
      </ul>
      {offsets.length < MAX_REMINDERS && available.length > 0 && (
        <select
          id={selectId}
          className="select"
          aria-label={t("settings.reminders.add")}
          disabled={disabled}
          value=""
          onChange={(e) => {
            const m = Number(e.target.value);
            if (m > 0) onChange([...offsets, m]);
          }}
        >
          <option value="">{t("settings.reminders.add")}</option>
          {available.map((m) => (
            <option key={m} value={m}>
              {offsetLabel(t, m)}
            </option>
          ))}
        </select>
      )}
    </div>
  );
}

interface CoursesProps {
  courses: readonly Course[];
  hidden: readonly string[];
  disabled: boolean;
  onToggle: (courseId: string, hidden: boolean) => void;
}

export function CourseSettings({ courses, hidden, disabled, onToggle }: CoursesProps) {
  const t = useT();
  return (
    <div className="settings__group">
      <p className="settings__label">{t("settings.courses")}</p>
      <p className="settings__hint">{t("settings.courses.hint")}</p>
      {courses.length === 0 ? (
        <p className="settings__hint">{t("settings.courses.empty")}</p>
      ) : (
        <ul className="course-list">
          {courses.map((c) => {
            const visible = !hidden.includes(c.id);
            const labelId = `course-${c.id}`;
            return (
              <li key={c.id} className="course-list__item">
                <span id={labelId} className="course-list__text">
                  <span className="course-list__code">
                    {c.code}
                    {c.source === "gradescope" && <span className="source-tag">Gradescope</span>}
                  </span>
                  {c.name !== c.code && <span className="course-list__name">{c.name}</span>}
                </span>
                <button
                  type="button"
                  role="switch"
                  className="switch"
                  aria-checked={visible}
                  aria-labelledby={labelId}
                  disabled={disabled}
                  onClick={() => onToggle(c.id, visible)}
                >
                  <span className="switch__thumb" />
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

interface UnsubmittableProps {
  checked: boolean;
  disabled: boolean;
  onChange: (show: boolean) => void;
}

export function UnsubmittableSetting({ checked, disabled, onChange }: UnsubmittableProps) {
  const t = useT();
  const labelId = useId();
  return (
    <div className="settings__group settings__row">
      <div>
        <p className="settings__label" id={labelId}>
          {t("settings.unsubmittable")}
        </p>
        <p className="settings__hint">{t("settings.unsubmittable.hint")}</p>
      </div>
      <button
        type="button"
        role="switch"
        className="switch"
        aria-checked={checked}
        aria-labelledby={labelId}
        disabled={disabled}
        onClick={() => onChange(!checked)}
      >
        <span className="switch__thumb" />
      </button>
    </div>
  );
}
