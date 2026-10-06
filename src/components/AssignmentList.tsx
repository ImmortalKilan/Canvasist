import { useMemo } from "react";
import { describeError } from "../errors";
import { formatDue, formatTime } from "../format";
import { useT } from "../i18n/context";
import type { Locale } from "../i18n/translate";
import { api, type AppError, type CanvasSnapshot } from "../ipc";

interface Props {
  snapshot: CanvasSnapshot | null;
  locale: Locale;
  error: AppError | null;
}

export function AssignmentList({ snapshot, locale, error }: Props) {
  const t = useT();
  const courseCodes = useMemo(
    () => new Map(snapshot?.courses.map((c) => [c.id, c.code]) ?? []),
    [snapshot],
  );

  return (
    <section className="assignments" aria-label={t("list.label")}>
      {error && (
        <p className="form-error" role="alert">
          {describeError(t, error)}
        </p>
      )}
      {!snapshot && !error && <p className="form-hint">{t("list.loading")}</p>}
      {snapshot && snapshot.assignments.length === 0 && (
        <p className="form-hint">{t("list.empty")}</p>
      )}
      {snapshot && snapshot.assignments.length > 0 && (
        <ul className="assignment-list">
          {snapshot.assignments.map((a) => {
            const url = a.url;
            return (
              <li key={a.id}>
                <button
                  type="button"
                  className="assignment"
                  disabled={!url}
                  title={t("list.openInCanvas")}
                  onClick={() => {
                    if (url) void api.openExternal(url);
                  }}
                >
                  <span className={`status-dot status-dot--${a.status}`} aria-hidden="true" />
                  <span className="assignment__main">
                    <span className="assignment__course">{courseCodes.get(a.courseId)}</span>
                    <span className="assignment__title">{a.title}</span>
                  </span>
                  <span className="assignment__meta">
                    <span className="assignment__due">{formatDue(a.dueAt, locale)}</span>
                    <span className={`badge badge--${a.status}`}>{t(`status.${a.status}`)}</span>
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
      {snapshot && (
        <p className="assignments__updated">
          {t("list.updated", { time: formatTime(snapshot.fetchedAt, locale) })}
        </p>
      )}
    </section>
  );
}
