import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { describeError } from "../errors";
import { formatDue, formatTime } from "../format";
import {
  FOLDED_GROUPS,
  GROUP_ORDER,
  NO_MARKS,
  groupAssignments,
  isManuallyDone,
  type GroupKey,
} from "../grouping";
import { useT } from "../i18n/context";
import type { Locale } from "../i18n/translate";
import { api, toAppError, type AppError, type Assignment, type Marks, type Snapshot } from "../ipc";
import { ConfirmDialog } from "./ConfirmDialog";
import { RowMenu, type MenuAction } from "./RowMenu";

interface Props {
  snapshot: Snapshot | null;
  locale: Locale;
  error: AppError | null;
}

export function AssignmentList({ snapshot, locale, error }: Props) {
  const t = useT();
  const [marks, setMarks] = useState<Marks>(NO_MARKS);
  const [markError, setMarkError] = useState<AppError | null>(null);
  const [expanded, setExpanded] = useState<ReadonlySet<GroupKey>>(new Set());
  const [confirming, setConfirming] = useState<Assignment | null>(null);

  useEffect(() => {
    let cancelled = false;
    api.getMarks().then(
      (m) => {
        if (!cancelled) setMarks(m);
      },
      (e: unknown) => {
        if (!cancelled) setMarkError(toAppError(e));
      },
    );
    return () => {
      cancelled = true;
    };
  }, []);

  const applyMark = useCallback(async (action: () => Promise<Marks>) => {
    setMarkError(null);
    try {
      setMarks(await action());
    } catch (e) {
      setMarkError(toAppError(e));
    }
  }, []);

  const courseCodes = useMemo(
    () => new Map(snapshot?.courses.map((c) => [c.id, c.code]) ?? []),
    [snapshot],
  );
  const groups = useMemo(
    () => (snapshot ? groupAssignments(snapshot.assignments, new Date(), marks) : null),
    [snapshot, marks],
  );

  const openCount = groups
    ? GROUP_ORDER.filter((k) => !FOLDED_GROUPS.has(k)).reduce(
        (n, k) => n + (groups.get(k)?.length ?? 0),
        0,
      )
    : 0;

  function actionsFor(a: Assignment, group: GroupKey): MenuAction[] {
    const actions: MenuAction[] = [];
    if (a.url) {
      const url = a.url;
      actions.push({ label: t("menu.open"), onSelect: () => void api.openExternal(url) });
    }
    const manual = isManuallyDone(a, marks);
    if (group === "dismissed" || manual) {
      actions.push({
        label: t("menu.restore"),
        onSelect: () => void applyMark(() => api.restore(a.id)),
      });
    } else if (group !== "completed") {
      actions.push({ label: t("menu.markDone"), onSelect: () => setConfirming(a) });
      if (group === "overdue") {
        actions.push({
          label: t("menu.dismiss"),
          onSelect: () => void applyMark(() => api.dismiss(a.id)),
        });
      }
    }
    return actions;
  }

  const closeDialog = useCallback(() => setConfirming(null), []);

  function toggle(key: GroupKey) {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

  return (
    <section className="assignments" aria-label={t("list.label")}>
      {[error, markError].map(
        (e, i) =>
          e && (
            <p key={i} className="form-error" role="alert">
              {describeError(t, e)}
            </p>
          ),
      )}
      {!snapshot && !error && <p className="form-hint">{t("list.loading")}</p>}
      {snapshot && snapshot.assignments.length === 0 && (
        <p className="form-hint">{t("list.empty")}</p>
      )}
      {snapshot && snapshot.assignments.length > 0 && openCount === 0 && (
        <p className="form-hint">{t("list.allDone")}</p>
      )}

      {groups &&
        GROUP_ORDER.map((key) => {
          const items = groups.get(key) ?? [];
          if (items.length === 0) return null;
          const rows = (
            <ul className="assignment-list">
              {items.map((a) => (
                <Row
                  key={a.id}
                  assignment={a}
                  group={key}
                  manual={isManuallyDone(a, marks)}
                  courseCode={courseCodes.get(a.courseId)}
                  locale={locale}
                  actions={actionsFor(a, key)}
                />
              ))}
            </ul>
          );
          if (!FOLDED_GROUPS.has(key)) {
            return (
              <Group key={key} title={t(`group.${key}`)} count={items.length}>
                {rows}
              </Group>
            );
          }
          const open = expanded.has(key);
          const listId = `group-${key}`;
          return (
            <div key={key} className="group">
              <button
                type="button"
                className="group__header group__header--toggle"
                aria-expanded={open}
                aria-controls={listId}
                onClick={() => toggle(key)}
              >
                <span className={`chevron${open ? " chevron--open" : ""}`} aria-hidden="true" />
                {t(`group.${key}`)}
                <span className="group__count">{items.length}</span>
              </button>
              {open && <div id={listId}>{rows}</div>}
            </div>
          );
        })}

      {snapshot && (
        <p className="assignments__updated">
          {t("list.updated", { time: formatTime(snapshot.fetchedAt, locale) })}
        </p>
      )}

      {confirming && (
        <ConfirmDialog
          title={t("confirm.markDone.title")}
          body={t("confirm.markDone.body", { title: confirming.title })}
          confirmLabel={t("confirm.markDone.yes")}
          cancelLabel={t("confirm.cancel")}
          onCancel={closeDialog}
          onConfirm={() => {
            const id = confirming.id;
            setConfirming(null);
            void applyMark(() => api.markDone(id));
          }}
        />
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

interface RowProps {
  assignment: Assignment;
  group: GroupKey;
  manual: boolean;
  courseCode: string | undefined;
  locale: Locale;
  actions: readonly MenuAction[];
}

function Row({ assignment: a, group, manual, courseCode, locale, actions }: RowProps) {
  const t = useT();
  // Today and tomorrow already name the day, so only the time is shown.
  const timeOnly = group === "today" || group === "tomorrow";
  const folded = FOLDED_GROUPS.has(group);
  const statusKey = manual ? "manualDone" : a.status;
  const url = a.url;
  return (
    <li className={`assignment${folded ? " assignment--done" : ""}`}>
      <button
        type="button"
        className="assignment__open"
        disabled={!url}
        title={a.source === "gradescope" ? t("list.openInGradescope") : t("list.openInCanvas")}
        onClick={() => {
          if (url) void api.openExternal(url);
        }}
      >
        <span className={`status-dot status-dot--${statusKey}`} aria-hidden="true" />
        <span className="assignment__main">
          <span className="assignment__course">
            {courseCode}
            {a.source === "gradescope" && <span className="source-tag">Gradescope</span>}
          </span>
          <span className="assignment__title">{a.title}</span>
        </span>
        <span className="assignment__meta">
          <span className="assignment__due">
            {timeOnly ? formatTime(a.dueAt, locale) : formatDue(a.dueAt, locale)}
          </span>
          {a.lateDueAt && !folded && (
            <span className="assignment__late">
              {t("list.lateDue", { date: formatDue(a.lateDueAt, locale) })}
            </span>
          )}
          {a.canvasDueAt && !folded && (
            <span className="assignment__note">
              {t("list.canvasDueDiffers", { date: formatDue(a.canvasDueAt, locale) })}
            </span>
          )}
          <span className={`badge badge--${statusKey}`}>{t(`status.${statusKey}`)}</span>
        </span>
      </button>
      {actions.length > 0 && <RowMenu actions={actions} />}
    </li>
  );
}
