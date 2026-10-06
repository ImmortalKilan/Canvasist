import { useEffect, useId, useRef, useState } from "react";
import { useT } from "../i18n/context";

export interface MenuAction {
  label: string;
  onSelect: () => void;
}

/** The "⋯" button on an assignment row and its small action menu. */
export function RowMenu({ actions }: { actions: readonly MenuAction[] }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const menuId = useId();
  const rootRef = useRef<HTMLDivElement>(null);
  const firstItemRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    firstItemRef.current?.focus();
    const onPointer = (e: PointerEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className="row-menu" ref={rootRef}>
      <button
        type="button"
        className={`row-menu__button${open ? " row-menu__button--open" : ""}`}
        aria-label={t("menu.more")}
        title={t("menu.more")}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        onClick={() => setOpen((v) => !v)}
      >
        <span aria-hidden="true">⋯</span>
      </button>
      {open && (
        <div className="row-menu__list" role="menu" id={menuId}>
          {actions.map((action, i) => (
            <button
              key={action.label}
              ref={i === 0 ? firstItemRef : undefined}
              type="button"
              role="menuitem"
              className="row-menu__item"
              onClick={() => {
                setOpen(false);
                action.onSelect();
              }}
            >
              {action.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
