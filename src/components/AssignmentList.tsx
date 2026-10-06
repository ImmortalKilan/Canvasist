import { useMemo, useState, type ReactNode } from "react";
import { describeError } from "../errors";
import { formatDue, formatTime } from "../format";
import { GROUP_ORDER, groupAssignments, type GroupKey } from "../grouping";
import { useT } from "../i18n/context";
import type { Locale } from "../i18n/translate";
import { api, type AppError, type Assignment, type Snapshot } from "../ipc";

interface Props {
  snapshot: Snapshot | null;
  locale: Locale;
  error: AppError | null;
}

export function AssignmentList({ snapshot, locale, error }: Props) {
  const t = useT();
  const [showCompleted, setShowCompleted] = useState(false);
  const courseCodes = useMemo(
    () => new Map(snapshot?.courses.map((c) => [c.id, c.code]) ?? []),
    [snapshot],
  );
  // Regrouped whenever new data arrives, so items move between groups after each refresh.
  const groups = useMemo(
    () => (snapshot ? groupAssignments(snapshot.assignments, new Date()) : null),
    [snapshot],
  );

  const completed = groups?.get("completed") ?? [];
  const openCount = snapshot ? snapshot.assignments.length - completed.length : 0;

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
      {snapshot && snapshot.assignments.length > 0 && openCount === 0 && (
        <p className="form-hint">{t("list.allDone")}</p>
      )}

      {groups &&
        GROUP_ORDER.filter((key) => key !== "completed").map((key) => {
          const items = groups.get(key) ?? [];
          if (items.length === 0) return null;
          return (
            <Group key={key} title={t(`group.${key}`)} count={items.length}>
              <Rows items={items} group={key} courseCodes={courseCodes} locale={locale} />
            </Group>
          );
        })}

      {completed.length > 0 && (
        <div className="group">
          <button
            type="button"
            className="group__header group__header--toggle"
            aria-expanded={showCompleted}
            aria-controls="completed-list"
            onClick={() => setShowCompleted((v) => !v)}
          >
            <span
              className={`chevron${showCompleted ? " chevron--open" : ""}`}
              aria-hidden="true"
            />
            {t("group.completed")}
            <span className="group__count">{completed.length}</span>
          </button>
          {showCompleted && (
            <div id="completed-list">
              <Rows items={completed} group="completed" courseCodes={courseCodes} locale={locale} />
            </div>
          )}
        </div>
      )}

      {snapshot && (
        <p className="assignments__updated">
          {t("list.updated", { time: formatTime(snapshot.fetchedAt, locale) })}
        </p>
      )}
    </section>
  );
}

function Group({ title, count, children }: { title: string; count: number; children: ReactNode }) {
  return (
    <div className="group">
      <h3 className="group__header">
        {title}
        <span className="group__count">{count}</span>
      </h3>
      {children}
    </div>
  );
}

interface RowsProps {
  items: readonly Assignment[];
  group: GroupKey;
  courseCodes: ReadonlyMap<string, string>;
  locale: Locale;
}

function Rows({ items, group, courseCodes, locale }: RowsProps) {
  const t = useT();
  // Today and tomorrow already name the day, so only the time is shown.
  const timeOnly = group === "today" || group === "tomorrow";
  return (
    <ul className="assignment-list">
      {items.map((a) => {
        const url = a.url;
        return (
          <li key={a.id}>
            <button
              type="button"
              className={`assignment${group === "completed" ? " assignment--done" : ""}`}
              disabled={!url}
              title={
                a.source === "gradescope" ? t("list.openInGradescope") : t("list.openInCanvas")
              }
              onClick={() => {
                if (url) void api.openExternal(url);
              }}
            >
              <span className={`status-dot status-dot--${a.status}`} aria-hidden="true" />
              <span className="assignment__main">
                <span className="assignment__course">
                  {courseCodes.get(a.courseId)}
                  {a.source === "gradescope" && <span className="source-tag">Gradescope</span>}
                </span>
                <span className="assignment__title">{a.title}</span>
              </span>
              <span className="assignment__meta">
                <span className="assignment__due">
                  {timeOnly ? formatTime(a.dueAt, locale) : formatDue(a.dueAt, locale)}
                </span>
                {a.lateDueAt && group !== "completed" && (
                  <span className="assignment__late">
                    {t("list.lateDue", { date: formatDue(a.lateDueAt, locale) })}
                  </span>
                )}
                <span className={`badge badge--${a.status}`}>{t(`status.${a.status}`)}</span>
              </span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}
