import { useEffect, useId, useRef } from "react";

interface Props {
  title: string;
  body: string;
  confirmLabel: string;
  cancelLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
}

/**
 * Modal confirmation. Focus starts on Cancel so that pressing Enter by habit
 * never confirms by accident (the lighthouse asks for a double confirmation).
 */
export function ConfirmDialog({
  title,
  body,
  confirmLabel,
  cancelLabel,
  onConfirm,
  onCancel,
}: Props) {
  const titleId = useId();
  const bodyId = useId();
  const cancelRef = useRef<HTMLButtonElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    cancelRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onCancel();
        return;
      }
      // Keep keyboard focus inside the dialog while it is open.
      if (e.key !== "Tab") return;
      const buttons = Array.from(dialogRef.current?.querySelectorAll("button") ?? []);
      const first = buttons[0];
      const last = buttons[buttons.length - 1];
      if (!first || !last) return;
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
      previous?.focus();
    };
  }, [onCancel]);

  return (
    <div
      className="dialog-backdrop"
      onPointerDown={(e) => e.target === e.currentTarget && onCancel()}
    >
      <div
        ref={dialogRef}
        className="dialog"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={bodyId}
      >
        <h2 id={titleId} className="dialog__title">
          {title}
        </h2>
        <p id={bodyId} className="dialog__body">
          {body}
        </p>
        <div className="dialog__actions">
          <button ref={cancelRef} type="button" className="secondary-button" onClick={onCancel}>
            {cancelLabel}
          </button>
          <button type="button" className="primary-button" onClick={onConfirm}>
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
