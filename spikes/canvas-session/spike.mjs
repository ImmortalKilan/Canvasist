// Technical spike: reach Canvas and Gradescope using an interactive Canvas login
// session instead of an access token (for schools that forbid student tokens).
//
// What it verifies:
//   1. A user can log in to Canvas (SSO / MFA) in a browser window we control.
//   2. The Canvas REST API accepts that session for assignments + submission status.
//   3. A Gradescope session can be obtained through Canvas's LTI launch.
//   4. Gradescope course and assignment pages expose due dates and status.
//   5. The exported session cookies also work from a plain HTTP client, which is
//      how the real app will refresh data in the background without a browser.
//
// Privacy rules:
//   - The user types credentials into the school's own login page; this script
//     never reads them.
//   - Output contains only counts, HTTP status codes, hostnames, cookie names and
//     lifetimes, and generic UI labels. No course names, assignment titles, IDs,
//     cookie values or URLs with query strings.
//   - The browser runs on a throwaway profile that Playwright deletes on exit.
//     Nothing is written to disk by this script.
//
// Usage:  node spike.mjs https://canvas.yourschool.edu

import { chromium } from "playwright-core";

const LOGIN_TIMEOUT_MS = 10 * 60 * 1000;
const LAUNCH_TIMEOUT_MS = 60 * 1000;
const GRADESCOPE_HOST = /(^|\.)gradescope\.(com|ca|eu)$/i;

// ---------- helpers ----------

function log(step, detail = "") {
  console.log(`[${step}] ${detail}`);
}

class SpikeError extends Error {}

function parseCanvasOrigin() {
  const raw = process.argv[2];
  if (!raw) throw new SpikeError("Usage: node spike.mjs https://canvas.yourschool.edu");
  const url = new URL(raw);
  if (url.protocol !== "https:") throw new SpikeError("Canvas URL must use https.");
  return url.origin;
}

function countBy(map, key) {
  map.set(key, (map.get(key) || 0) + 1);
}

function printCounts(step, label, map) {
  const parts = [...map.entries()].sort((a, b) => b[1] - a[1]).map(([k, v]) => `${k}=${v}`);
  log(step, `${label}: ${parts.join(", ") || "(none)"}`);
}

function cookieMatches(host, cookieDomain) {
  const d = cookieDomain.replace(/^\./, "").toLowerCase();
  return host === d || host.endsWith("." + d);
}

function describeLifetime(cookie) {
  if (cookie.expires === -1) return "session";
  const days = (cookie.expires * 1000 - Date.now()) / 86_400_000;
  return days < 1 ? `${Math.round(days * 24)}h` : `${Math.round(days)}d`;
}

// GET a Canvas API path using the browser context's cookies, following pagination.
async function canvasGet(ctx, origin, path) {
  const items = [];
  let url = new URL(path, origin);
  for (;;) {
    if (url.origin !== origin) throw new SpikeError("Pagination left the Canvas origin.");
    const res = await ctx.request.get(url.href, {
      headers: { Accept: "application/json" },
      maxRedirects: 0,
    });
    if (res.status() !== 200) return { ok: false, status: res.status(), data: items };
    // Session-authenticated Canvas responses may carry an anti-JSON-hijacking prefix.
    const body = (await res.text()).replace(/^while\(1\);/, "");
    const data = JSON.parse(body);
    if (!Array.isArray(data)) return { ok: true, status: 200, data };
    items.push(...data);
    const next = (res.headers()["link"] || "").match(/<([^>]+)>;\s*rel="next"/);
    if (!next) return { ok: true, status: 200, data: items };
    url = new URL(next[1]);
  }
}

// ---------- step 1: interactive login ----------

async function waitForLogin(page, origin) {
  log("1 login", "An Edge window has opened. Please log in to Canvas there.");
  log("1 login", `Waiting up to ${LOGIN_TIMEOUT_MS / 60000} minutes...`);
  await page.goto(`${origin}/login`);
  // Logged in = back on the Canvas origin, outside the /login flow.
  await page.waitForURL(
    (u) => u.origin === origin && !u.pathname.startsWith("/login"),
    { timeout: LOGIN_TIMEOUT_MS },
  );
  await page.waitForLoadState("domcontentloaded");
}

// ---------- step 2/3: Canvas data ----------

async function inspectCanvas(ctx, origin) {
  const self = await canvasGet(ctx, origin, "/api/v1/users/self");
  if (!self.ok) throw new SpikeError(`Canvas API rejected the session (HTTP ${self.status}).`);
  log("2 canvas", "API accepts the login session: OK");

  const courses = await canvasGet(
    ctx,
    origin,
    "/api/v1/courses?enrollment_state=active&include[]=term&per_page=100",
  );
  if (!courses.ok) throw new SpikeError(`Course list failed (HTTP ${courses.status}).`);

  const now = Date.now();
  const termBuckets = new Map();
  for (const c of courses.data) {
    const t = c.term || {};
    const start = t.start_at ? Date.parse(t.start_at) : null;
    const end = t.end_at ? Date.parse(t.end_at) : null;
    let bucket;
    if (start === null && end === null) bucket = "term-without-dates";
    else if ((start === null || start <= now) && (end === null || now <= end)) bucket = "current-by-dates";
    else if (start !== null && start > now) bucket = "future";
    else bucket = "past";
    countBy(termBuckets, bucket);
  }
  log("2 canvas", `${courses.data.length} active course(s)`);
  printCounts("2 canvas", "term classification", termBuckets);

  const submissionTypes = new Map();
  const submissionStates = new Map();
  const flags = new Map();
  const gradescopeTabs = [];
  const gradescopeAssignments = [];
  let totalAssignments = 0;
  let withDue = 0;

  for (const c of courses.data) {
    const tabs = await canvasGet(ctx, origin, `/api/v1/courses/${c.id}/tabs`);
    if (tabs.ok) {
      for (const t of tabs.data) {
        if (t.type === "external" && /gradescope/i.test(t.label || "")) {
          gradescopeTabs.push({ courseId: c.id, toolId: String(t.id).replace("context_external_tool_", ""), htmlUrl: t.html_url });
        }
      }
    }

    const asg = await canvasGet(
      ctx,
      origin,
      `/api/v1/courses/${c.id}/assignments?include[]=submission&per_page=100`,
    );
    if (!asg.ok) {
      countBy(flags, `assignments-http-${asg.status}`);
      continue;
    }
    for (const a of asg.data) {
      totalAssignments++;
      if (a.due_at) withDue++;
      for (const st of a.submission_types || []) countBy(submissionTypes, st);
      const s = a.submission;
      if (s) {
        countBy(submissionStates, s.workflow_state || "unknown");
        if (s.late) countBy(flags, "late");
        if (s.missing) countBy(flags, "missing");
        if (s.excused) countBy(flags, "excused");
        if (s.submitted_at) countBy(flags, "has-submitted_at");
      } else {
        countBy(submissionStates, "no-submission-object");
      }
      const toolUrl = a.external_tool_tag_attributes?.url;
      if (toolUrl) {
        let host = "";
        try { host = new URL(toolUrl).hostname; } catch { /* ignore malformed URL */ }
        if (GRADESCOPE_HOST.test(host)) gradescopeAssignments.push({ courseId: c.id, assignmentId: a.id });
      }
    }
  }

  log("3 canvas", `${totalAssignments} assignment(s), ${withDue} with a due date`);
  printCounts("3 canvas", "submission types", submissionTypes);
  printCounts("3 canvas", "submission workflow states", submissionStates);
  printCounts("3 canvas", "submission flags", flags);
  log("3 canvas", `${gradescopeTabs.length} course(s) with a Gradescope nav tab`);
  log("3 canvas", `${gradescopeAssignments.length} Canvas assignment(s) linking to Gradescope`);
  return { gradescopeTabs, gradescopeAssignments };
}

// ---------- step 4: Gradescope via LTI ----------

async function launchGradescope(ctx, page, origin, entry) {
  const attempts = [
    ...entry.gradescopeTabs.slice(0, 2).map((t) => ({
      kind: "nav-tab sessionless",
      path: `/api/v1/courses/${t.courseId}/external_tools/sessionless_launch?id=${t.toolId}&launch_type=course_navigation`,
    })),
    ...entry.gradescopeAssignments.slice(0, 2).map((a) => ({
      kind: "assignment sessionless",
      path: `/api/v1/courses/${a.courseId}/external_tools/sessionless_launch?launch_type=assessment&assignment_id=${a.assignmentId}`,
    })),
  ];

  for (const [i, at] of attempts.entries()) {
    const sl = await canvasGet(ctx, origin, at.path);
    if (!sl.ok || !sl.data?.url) {
      log("4 launch", `#${i + 1} ${at.kind}: sessionless_launch HTTP ${sl.status}`);
      continue;
    }
    try {
      await page.goto(sl.data.url);
      await page.waitForURL((u) => GRADESCOPE_HOST.test(u.hostname), { timeout: LAUNCH_TIMEOUT_MS });
      await page.waitForLoadState("domcontentloaded");
      log("4 launch", `#${i + 1} ${at.kind}: landed on ${new URL(page.url()).hostname}`);
      return new URL(page.url()).origin;
    } catch {
      log("4 launch", `#${i + 1} ${at.kind}: did not reach Gradescope (ended on ${new URL(page.url()).hostname})`);
    }
  }

  // Fallback: open the Gradescope course tab the way a user would click it.
  for (const [i, t] of entry.gradescopeTabs.slice(0, 2).entries()) {
    const popupPromise = ctx.waitForEvent("page", { timeout: LAUNCH_TIMEOUT_MS }).catch(() => null);
    await page.goto(t.htmlUrl);
    const deadline = Date.now() + LAUNCH_TIMEOUT_MS;
    while (Date.now() < deadline) {
      const gsFrame = page.frames().find((f) => {
        try { return GRADESCOPE_HOST.test(new URL(f.url()).hostname); } catch { return false; }
      });
      if (gsFrame) {
        log("4 launch", `tab fallback #${i + 1}: Gradescope loaded inside an iframe`);
        return new URL(gsFrame.url()).origin;
      }
      await page.waitForTimeout(1000);
    }
    const popup = await popupPromise;
    if (popup && GRADESCOPE_HOST.test(new URL(popup.url()).hostname)) {
      log("4 launch", `tab fallback #${i + 1}: Gradescope opened in a new window`);
      return new URL(popup.url()).origin;
    }
    log("4 launch", `tab fallback #${i + 1}: no Gradescope frame or window detected`);
  }
  return null;
}

// ---------- step 5: Gradescope data ----------

async function inspectGradescope(page, gsOrigin) {
  await page.goto(`${gsOrigin}/account`);
  await page.waitForLoadState("domcontentloaded");
  const accountPath = new URL(page.url()).pathname;
  if (accountPath.startsWith("/login")) {
    log("5 gradescope", "/account redirected to login: Gradescope session NOT usable at top level");
    return;
  }

  // Group course links by the term heading that precedes them (term labels like
  // "Fall 2026" are generic, not personal).
  const terms = await page.evaluate(() => {
    const result = [];
    let current = { term: "(ungrouped)", courses: [] };
    const nodes = document.querySelectorAll(".courseList--term, a.courseBox, a[href^='/courses/']");
    const seen = new Set();
    for (const n of nodes) {
      if (n.classList.contains("courseList--term")) {
        if (current.courses.length) result.push(current);
        current = { term: n.textContent.trim().slice(0, 40), courses: [] };
        continue;
      }
      const href = n.getAttribute("href");
      if (/^\/courses\/\d+$/.test(href) && !seen.has(href)) {
        seen.add(href);
        current.courses.push(href);
      }
    }
    if (current.courses.length) result.push(current);
    return result;
  });
  for (const t of terms) log("5 gradescope", `term "${t.term}": ${t.courses.length} course(s)`);
  if (terms.length === 0) {
    log("5 gradescope", "no course links found on /account");
    return;
  }

  // Inspect every course in the most recent term (listed first by Gradescope).
  const statusLabels = new Map();
  const timeClasses = new Map();
  let rows = 0;
  let rowsWithDue = 0;
  let rowsWithLateDue = 0;
  for (const href of terms[0].courses) {
    await page.goto(`${gsOrigin}${href}`);
    await page.waitForLoadState("domcontentloaded");
    const summary = await page.evaluate(() => {
      const out = { rows: 0, withDue: 0, withLateDue: 0, statuses: [], timeClasses: [] };
      const trs = document.querySelectorAll("#assignments-student-table tbody tr");
      for (const tr of trs) {
        out.rows++;
        const times = tr.querySelectorAll("time[datetime]");
        let due = false;
        let late = false;
        for (const t of times) {
          out.timeClasses.push(t.className || "(no-class)");
          if (/dueDate/i.test(t.className) && !/late/i.test(t.className)) due = true;
          if (/late/i.test(t.className)) late = true;
        }
        if (due) out.withDue++;
        if (late) out.withLateDue++;
        // Only read the dedicated status cell; never fall back to cells that may hold titles.
        const statusCell = tr.querySelector(".submissionStatus");
        let label = (statusCell?.textContent || "").replace(/\s+/g, " ").trim();
        // Collapse scores so no grades are printed.
        label = label.replace(/\d+(\.\d+)?\s*\/\s*\d+(\.\d+)?/g, "<score>").slice(0, 40);
        out.statuses.push(statusCell ? label || "(empty)" : "(no .submissionStatus cell)");
      }
      return out;
    });
    rows += summary.rows;
    rowsWithDue += summary.withDue;
    rowsWithLateDue += summary.withLateDue;
    for (const s of summary.statuses) countBy(statusLabels, s);
    for (const c of summary.timeClasses) countBy(timeClasses, c);
  }
  log("5 gradescope", `current term: ${rows} assignment row(s), ${rowsWithDue} with due date, ${rowsWithLateDue} with late due date`);
  printCounts("5 gradescope", "status labels", statusLabels);
  printCounts("5 gradescope", "<time> classes", timeClasses);
}

// ---------- step 6: plain HTTP client with exported cookies ----------

async function checkPlainHttp(ctx, origin, gsOrigin) {
  const cookies = await ctx.cookies();
  const headerFor = (url) =>
    cookies
      .filter((c) => cookieMatches(url.hostname, c.domain) && url.pathname.startsWith(c.path))
      .map((c) => `${c.name}=${c.value}`)
      .join("; ");

  const targets = [new URL("/api/v1/users/self", origin)];
  if (gsOrigin) targets.push(new URL("/account", gsOrigin));
  for (const url of targets) {
    const res = await fetch(url, {
      headers: { Cookie: headerFor(url), Accept: "application/json, text/html" },
      redirect: "manual",
    });
    const location = res.headers.get("location");
    const where = location ? ` -> redirect to ${new URL(location, url).pathname.split("/")[1] || "/"}` : "";
    log("6 plain-http", `${url.hostname}${url.pathname}: HTTP ${res.status}${where}`);
  }

  // Cookie lifetimes tell us how long a saved session might last.
  for (const host of [new URL(origin).hostname, gsOrigin && new URL(gsOrigin).hostname].filter(Boolean)) {
    const relevant = cookies.filter((c) => cookieMatches(host, c.domain));
    const desc = relevant.map((c) => `${c.name}(${describeLifetime(c)}${c.httpOnly ? ",httpOnly" : ""})`);
    log("6 cookies", `${host}: ${desc.join(", ") || "(none)"}`);
  }
}

// ---------- main ----------

async function main() {
  const origin = parseCanvasOrigin();
  log("0 config", `Canvas host: ${new URL(origin).hostname}`);

  // Non-persistent context: Playwright uses a temporary profile and deletes it on close.
  const browser = await chromium.launch({ channel: "msedge", headless: false });
  try {
    const ctx = await browser.newContext();
    const page = await ctx.newPage();

    await waitForLogin(page, origin);
    const entry = await inspectCanvas(ctx, origin);

    let gsOrigin = null;
    if (entry.gradescopeTabs.length + entry.gradescopeAssignments.length === 0) {
      log("4 launch", "no Gradescope entry point found in any active course");
    } else {
      gsOrigin = await launchGradescope(ctx, page, origin, entry);
      if (gsOrigin) await inspectGradescope(page, gsOrigin);
      else log("4 launch", "FAILED to obtain a Gradescope session");
    }

    await checkPlainHttp(ctx, origin, gsOrigin);
    log("done", "Spike finished. Closing browser and deleting the temporary profile.");
  } finally {
    await browser.close();
  }
}

main().catch((e) => {
  // Only print our own messages or the error class, never raw page content.
  const raw = String(e?.message || "").split("\n")[0].slice(0, 200);
  // Strip query strings: launch URLs carry one-time verifiers.
  const safe = raw.replace(/\?[^\s"']*/g, "?<redacted>");
  const msg = e instanceof SpikeError ? e.message : `${e?.name || "Error"}: ${safe}`;
  console.error(`FAIL: ${msg}`);
  process.exit(1);
});
