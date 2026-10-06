// Technical spike: can we reach a student's Gradescope data using only a
// Canvas access token, via Canvas "sessionless launch" of the Gradescope LTI tool?
//
// Privacy rules for this script:
//   - The token is read from .secrets/canvas.env and is never printed.
//   - The bearer token is only ever sent to the configured Canvas host.
//   - Output contains only step names, HTTP status codes, hostnames and counts.
//     No course names, assignment titles, IDs, URLs with query strings or cookies.
//
// Usage (from project root):  node spikes/gradescope-lti/spike.mjs

import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const MAX_HOPS = 20;
const GRADESCOPE_HOST = /(^|\.)gradescope\.(com|ca|eu)$/i;

// ---------- config ----------

function loadEnv() {
  const file = resolve(process.cwd(), ".secrets/canvas.env");
  let text;
  try {
    text = readFileSync(file, "utf8");
  } catch {
    fail(`Missing ${file}. Copy canvas.env.example to canvas.env and fill it in.`);
  }
  const env = {};
  for (const line of text.split(/\r?\n/)) {
    const m = line.match(/^\s*([A-Z_]+)\s*=\s*(.*?)\s*$/);
    if (m) env[m[1]] = m[2];
  }
  if (!env.CANVAS_BASE_URL || !env.CANVAS_TOKEN || env.CANVAS_TOKEN.includes("paste-your")) {
    fail("CANVAS_BASE_URL and CANVAS_TOKEN must be set in .secrets/canvas.env");
  }
  const base = new URL(env.CANVAS_BASE_URL);
  if (base.protocol !== "https:") fail("CANVAS_BASE_URL must use https.");
  return { canvasOrigin: base.origin, token: env.CANVAS_TOKEN };
}

// ---------- logging ----------

function log(step, detail = "") {
  console.log(`[${step}] ${detail}`);
}

function fail(msg) {
  console.error(`FAIL: ${msg}`);
  process.exit(1);
}

// ---------- minimal cookie jar (host + domain matching only) ----------

class CookieJar {
  #cookies = []; // { name, value, domain, hostOnly, path }

  store(url, response) {
    const host = url.hostname.toLowerCase();
    for (const raw of response.headers.getSetCookie()) {
      const [pair, ...attrs] = raw.split(";");
      const eq = pair.indexOf("=");
      if (eq < 1) continue;
      const name = pair.slice(0, eq).trim();
      const value = pair.slice(eq + 1).trim();
      let domain = host;
      let hostOnly = true;
      let path = "/";
      let expired = false;
      for (const a of attrs) {
        const [k, v = ""] = a.split("=").map((s) => s.trim());
        const key = k.toLowerCase();
        if (key === "domain" && v) {
          const d = v.replace(/^\./, "").toLowerCase();
          // Reject cookies for unrelated domains.
          if (host === d || host.endsWith("." + d)) {
            domain = d;
            hostOnly = false;
          }
        } else if (key === "path" && v.startsWith("/")) {
          path = v;
        } else if (key === "max-age" && Number(v) <= 0) {
          expired = true;
        } else if (key === "expires" && Date.parse(v) < Date.now()) {
          expired = true;
        }
      }
      this.#cookies = this.#cookies.filter(
        (c) => !(c.name === name && c.domain === domain && c.path === path),
      );
      if (!expired) this.#cookies.push({ name, value, domain, hostOnly, path });
    }
  }

  header(url) {
    const host = url.hostname.toLowerCase();
    return this.#cookies
      .filter((c) =>
        (c.hostOnly ? host === c.domain : host === c.domain || host.endsWith("." + c.domain)) &&
        url.pathname.startsWith(c.path),
      )
      .map((c) => `${c.name}=${c.value}`)
      .join("; ");
  }

  countFor(pattern) {
    return this.#cookies.filter((c) => pattern.test(c.domain)).length;
  }
}

// ---------- HTML helpers ----------

function decodeEntities(s) {
  return s
    .replace(/&quot;/g, '"')
    .replace(/&#39;|&#x27;/g, "'")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&#x2F;|&#47;/g, "/")
    .replace(/&#(\d+);/g, (_, n) => String.fromCharCode(Number(n)))
    .replace(/&amp;/g, "&");
}

function attr(tag, name) {
  const m = tag.match(new RegExp(`\\s${name}\\s*=\\s*("([^"]*)"|'([^']*)')`, "i"));
  return m ? decodeEntities(m[2] ?? m[3]) : null;
}

// Find the first <form> on the page (LTI launches use a single auto-submit form).
function extractForm(html, pageUrl) {
  const formMatch = html.match(/<form\b[^>]*>([\s\S]*?)<\/form>/i);
  if (!formMatch) return null;
  const openTag = formMatch[0].match(/<form\b[^>]*>/i)[0];
  const action = attr(openTag, "action");
  if (!action) return null;
  const method = (attr(openTag, "method") || "get").toLowerCase();
  const fields = new URLSearchParams();
  for (const input of formMatch[1].matchAll(/<input\b[^>]*>/gi)) {
    const name = attr(input[0], "name");
    if (name) fields.append(name, attr(input[0], "value") ?? "");
  }
  return { action: new URL(action, pageUrl), method, fields };
}

// ---------- HTTP ----------

const jar = new CookieJar();
const UA = "Canvasist-spike/0.1";

async function request(url, { method = "GET", body, headers = {} } = {}) {
  const h = { "User-Agent": UA, ...headers };
  const cookie = jar.header(url);
  if (cookie) h.Cookie = cookie;
  const res = await fetch(url, { method, body, headers: h, redirect: "manual" });
  jar.store(url, res);
  return res;
}

async function canvasApi(cfg, path) {
  const results = [];
  let url = new URL(path, cfg.canvasOrigin);
  while (url) {
    // Safety: bearer token goes to the Canvas origin only.
    if (url.origin !== cfg.canvasOrigin) fail("Refusing to send token to non-Canvas origin.");
    const res = await fetch(url, {
      headers: { Authorization: `Bearer ${cfg.token}`, "User-Agent": UA, Accept: "application/json" },
    });
    if (!res.ok) return { ok: false, status: res.status, data: results };
    const data = await res.json();
    if (!Array.isArray(data)) return { ok: true, status: res.status, data };
    results.push(...data);
    const next = (res.headers.get("link") || "").match(/<([^>]+)>;\s*rel="next"/);
    url = next ? new URL(next[1]) : null;
  }
  return { ok: true, status: 200, data: results };
}

// Follow redirects and auto-submit forms until we land on a Gradescope page.
async function followLaunch(startUrl) {
  let url = startUrl;
  let method = "GET";
  let body;
  const hops = [];
  for (let i = 0; i < MAX_HOPS; i++) {
    const headers = body ? { "Content-Type": "application/x-www-form-urlencoded" } : {};
    const res = await request(url, { method, body, headers });
    hops.push(`${method} ${url.hostname} -> ${res.status}`);

    if (res.status >= 300 && res.status < 400 && res.headers.get("location")) {
      url = new URL(res.headers.get("location"), url);
      method = "GET";
      body = undefined;
      continue;
    }
    const html = await res.text();
    const onGradescope = GRADESCOPE_HOST.test(url.hostname);
    const form = extractForm(html, url);
    // LTI hand-off forms cross hosts (Canvas -> Gradescope or back). Once on
    // Gradescope, any same-host form is page UI (e.g. search), not a launch step.
    const isLaunchForm = form && (!onGradescope || form.action.hostname !== url.hostname);
    if (!isLaunchForm) return { landed: onGradescope && res.ok, url, hops, html };
    url = form.action;
    if (form.method === "post") {
      method = "POST";
      body = form.fields.toString();
    } else {
      for (const [k, v] of form.fields) url.searchParams.append(k, v);
      method = "GET";
      body = undefined;
    }
  }
  return { landed: false, url, hops, html: "" };
}

// ---------- main ----------

async function main() {
  const cfg = loadEnv();
  log("config", `Canvas host: ${new URL(cfg.canvasOrigin).hostname}`);

  // 1. Token check
  const self = await canvasApi(cfg, "/api/v1/users/self");
  if (!self.ok) fail(`Token check failed (HTTP ${self.status}).`);
  log("1 token", "OK");

  // 2. Active courses with term info
  const courses = await canvasApi(
    cfg,
    "/api/v1/courses?enrollment_state=active&include[]=term&per_page=100",
  );
  if (!courses.ok) fail(`Course list failed (HTTP ${courses.status}).`);
  const terms = new Map();
  for (const c of courses.data) {
    const key = c.term?.name ?? "(no term)";
    terms.set(key, (terms.get(key) || 0) + 1);
  }
  log("2 courses", `${courses.data.length} active courses across ${terms.size} term(s)`);
  for (const [, n] of terms) log("2 courses", `  term bucket: ${n} course(s)`);

  // 3. Find Gradescope via course navigation tabs and via external-tool assignments
  const candidates = [];
  let gsAssignmentLinks = 0;
  for (const c of courses.data) {
    const tabs = await canvasApi(cfg, `/api/v1/courses/${c.id}/tabs`);
    if (tabs.ok) {
      for (const t of tabs.data) {
        if (t.type === "external" && /gradescope/i.test(t.label || "")) {
          const toolId = String(t.id).replace("context_external_tool_", "");
          candidates.push({ courseId: c.id, kind: "course_navigation", toolId });
        }
      }
    }
    const asg = await canvasApi(cfg, `/api/v1/courses/${c.id}/assignments?per_page=100`);
    if (asg.ok) {
      for (const a of asg.data) {
        const u = a.external_tool_tag_attributes?.url;
        if (u && GRADESCOPE_HOST.test(safeHost(u))) {
          gsAssignmentLinks++;
          candidates.push({ courseId: c.id, kind: "assessment", assignmentId: a.id });
        }
      }
    }
  }
  const navCount = candidates.filter((x) => x.kind === "course_navigation").length;
  log("3 discover", `${navCount} course(s) with a Gradescope nav tab`);
  log("3 discover", `${gsAssignmentLinks} Canvas assignment(s) linking to Gradescope`);
  if (candidates.length === 0) fail("No Gradescope entry point found in any active course.");

  // 4. Sessionless launch — try nav-tab candidates first, then assignment candidates
  candidates.sort((a, b) => (a.kind === b.kind ? 0 : a.kind === "course_navigation" ? -1 : 1));
  let launched = null;
  for (const [i, cand] of candidates.slice(0, 4).entries()) {
    const qs =
      cand.kind === "course_navigation"
        ? `id=${cand.toolId}&launch_type=course_navigation`
        : `launch_type=assessment&assignment_id=${cand.assignmentId}`;
    const sl = await canvasApi(
      cfg,
      `/api/v1/courses/${cand.courseId}/external_tools/sessionless_launch?${qs}`,
    );
    if (!sl.ok || !sl.data?.url) {
      log("4 launch", `candidate #${i + 1} (${cand.kind}): sessionless_launch HTTP ${sl.status}`);
      continue;
    }
    log("4 launch", `candidate #${i + 1} (${cand.kind}): got launch URL`);
    const result = await followLaunch(new URL(sl.data.url));
    for (const h of result.hops) log("4 launch", `  hop: ${h}`);
    log("4 launch", `  landed on Gradescope: ${result.landed}`);
    if (result.landed) {
      launched = result;
      break;
    }
  }
  if (!launched) fail("Could not establish a Gradescope session via sessionless launch.");
  log("4 launch", `Gradescope cookies held: ${jar.countFor(GRADESCOPE_HOST)}`);

  // 5. Use the session: list all Gradescope courses, then assignments in one course
  const gsOrigin = launched.url.origin;
  const account = await request(new URL("/account", gsOrigin));
  const accountHtml = account.status === 200 ? await account.text() : "";
  const courseLinks = [...accountHtml.matchAll(/href="(\/courses\/\d+)"/g)].map((m) => m[1]);
  const uniqueCourses = [...new Set(courseLinks)];
  log("5 gradescope", `/account HTTP ${account.status}, ${uniqueCourses.length} course link(s) visible`);

  if (uniqueCourses.length > 0) {
    const coursePage = await request(new URL(uniqueCourses[0], gsOrigin));
    const html = coursePage.status === 200 ? await coursePage.text() : "";
    const rows = (html.match(/<tr\b[^>]*role="row"/gi) || []).length;
    const times = (html.match(/<time\b[^>]*datetime=/gi) || []).length;
    const statuses = (html.match(/submissionStatus/gi) || []).length;
    log(
      "5 gradescope",
      `first course page HTTP ${coursePage.status}: ${rows} table row(s), ${times} <time> tag(s), ${statuses} status marker(s)`,
    );
  }

  log("done", "Spike finished.");
}

function safeHost(u) {
  try {
    return new URL(u).hostname;
  } catch {
    return "";
  }
}

main().catch((e) => fail(e?.message || String(e)));
