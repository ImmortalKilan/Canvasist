// Injected into the hidden Gradescope bridge window (see bridge.rs) before any
// page script runs. On the Canvas page that launches Gradescope, it opens the
// launch as the whole page instead of inside Canvas's frame. Gradescope then
// sets its cookies as a first-party site, which works even where cookies set
// inside a frame are blocked or kept apart (as on some WebView2 setups).
(() => {
  if (location.origin !== "__CANVAS_ORIGIN__") return;

  // Canvas names the launch form `tool_form` or `tool_form_<id>`.
  const isLaunchForm = (el) => el instanceof HTMLFormElement && /^tool_form(_|$)/.test(el.id);
  const nativeSubmit = HTMLFormElement.prototype.submit;

  // Canvas submits the form through jQuery, which ends in the native submit().
  HTMLFormElement.prototype.submit = function () {
    if (isLaunchForm(this)) this.target = "_self";
    return nativeSubmit.call(this);
  };
  // A button click or requestSubmit() fires a submit event instead.
  addEventListener(
    "submit",
    (event) => {
      if (isLaunchForm(event.target)) event.target.target = "_self";
    },
    true,
  );

  // A tool set to open in a new window waits for a click that never comes in
  // a hidden window, so launch it directly.
  document.addEventListener("DOMContentLoaded", () => {
    const form = document.querySelector("form[data-tool-launch-type='window']");
    if (form && isLaunchForm(form)) {
      form.target = "_self";
      nativeSubmit.call(form);
    }
  });
})();
