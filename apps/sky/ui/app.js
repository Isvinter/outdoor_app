// The user interface. It only displays what Rust reports and passes on what the user
// taps; filter, tone and track live in Rust and keep running while this page is paused
// (screen locked).

"use strict";

// Collected for the smoke test (see smoke.rs), which reports them.
window.scriptErrors = [];
window.addEventListener("error", (event) => window.scriptErrors.push(String(event.message)));
window.addEventListener("unhandledrejection", (event) =>
  window.scriptErrors.push(String(event.reason)),
);

const invoke = window.__TAURI__.core.invoke;
const $ = (id) => document.getElementById(id);

let running = false;
let testTone = null;
let testAlert = null; // warning sound under test: "near", "alarm" or null
let settings = null;
let activePage = "instruments";

// ---- Formatting (units are converted here only; Rust works in SI) ----

const number = (value, digits = 0) =>
  value === null || value === undefined ? "–" : value.toFixed(digits);

// Typographic minus, and never "-0.0".
function signed(value, digits = 1) {
  if (value === null || value === undefined) return "–";
  const rounded = Number(value.toFixed(digits));
  return (rounded > 0 ? "+" : rounded < 0 ? "−" : "") + Math.abs(rounded).toFixed(digits);
}

const COMPASS = ["N", "NO", "O", "SO", "S", "SW", "W", "NW"];
const compass = (degrees) => COMPASS[Math.round(degrees / 45) % 8];

// ---- Pages ----

function showPage(name) {
  activePage = name;
  for (const page of document.querySelectorAll(".page")) {
    page.classList.toggle("active", page.id === "page-" + name);
  }
  for (const tab of document.querySelectorAll("#tabs button")) {
    tab.classList.toggle("active", tab.dataset.page === name);
  }
  if (name === "map") {
    FlightMap.show();
    // Regions copied in meanwhile show up when the map is opened.
    updateMapStyle();
  }
  if (name === "settings") {
    updateMapStyle();
    refreshRegions();
    // Files copied in meanwhile are read.
    loadAirspaces();
  }
  if (name === "flights") loadFlights();
}

for (const tab of document.querySelectorAll("#tabs button")) {
  tab.addEventListener("click", () => showPage(tab.dataset.page));
}

// ---- Instruments ----

// Full deflection of the bar, in m/s.
const BAR_RANGE = 5;

// Expected longest normal pause per sensor; anything above is highlighted.
const sensorRows = [
  ["Beschleunigung", "accel", 0.5],
  ["Luftdruck", "pressure", 3],
  ["GPS", "gps", 3],
];

function render(status) {
  const climb = status?.climbRateMps ?? null;
  const climbThreshold = settings?.climbThresholdMps ?? 0.2;
  const sinkThreshold = settings?.sinkThresholdMps ?? -2.5;
  // Same thresholds as the tone: green while it beeps, red while the sink tone sounds.
  const state = climb === null ? "" : climb >= climbThreshold ? "up" : climb <= sinkThreshold ? "down" : "";

  $("climb").textContent = signed(climb);
  $("climb").className = state;

  const fill = $("bar-fill");
  const deflection = climb === null ? 0 : Math.min(Math.abs(climb) / BAR_RANGE, 1);
  fill.className = climb !== null && climb < 0 ? "down" : "up";
  fill.style.height = (deflection * 50).toFixed(1) + "%";

  $("altitude").textContent = number(status?.altitudeM);
  const fix = status?.fix;
  $("speed").textContent = fix?.groundSpeedMps == null ? "–" : (fix.groundSpeedMps * 3.6).toFixed(0);
  $("course").textContent = fix?.courseDeg == null ? "–" : fix.courseDeg.toFixed(0) + "°";
  $("course-label").textContent = fix?.courseDeg == null ? "Kurs" : "Kurs " + compass(fix.courseDeg);

  $("map-climb").textContent = signed(climb);
  $("map-altitude").textContent = number(status?.altitudeM);
  if (fix) FlightMap.setPosition(fix.latitudeDeg, fix.longitudeDeg, fix.courseDeg ?? null);

  renderRecording(status?.recording ?? null);
  renderWarnings(status?.airspaceWarnings ?? []);
  if (activePage === "settings") renderSensors(status);
}

// ---- Airspace warnings (decided in Rust; shown here) ----

const distance = (metres) =>
  metres >= 1000 ? (metres / 1000).toFixed(1).replace(".", ",") + " km" : Math.round(metres) + " m";

function warningDetail(warning) {
  const horizontal = warning.horizontalM;
  const inside = horizontal <= 0 && warning.verticalSide === "between";
  if (inside) return `Im Luftraum (${warning.lower} bis ${warning.upper})`;
  const parts = [horizontal <= 0 ? "Darunter oder darüber" : distance(horizontal) + " entfernt"];
  if (warning.verticalSide === "below") {
    parts.push(`${Math.round(warning.verticalM)} m unter ${warning.lower}`);
  } else if (warning.verticalSide === "above") {
    parts.push(`${Math.round(warning.verticalM)} m über ${warning.upper}`);
  } else {
    parts.push(`${warning.lower} bis ${warning.upper}`);
  }
  if (warning.entryInS > 0) parts.push(`Einflug in ${Math.round(warning.entryInS)} s`);
  return parts.join(" · ");
}

function renderWarnings(warnings) {
  FlightMap.setWarnings(warnings);
  const banner = $("warning");
  const first = warnings[0];
  banner.hidden = !first;
  // Room to scroll the page content out from under the banner.
  document.body.classList.toggle("warning", Boolean(first));
  if (!first) return;
  const kind = AIRSPACE_CATEGORIES.find(([category]) => category === first.category);
  const name = first.name || "(ohne Namen)";
  // "CTR München" needs no "CTR" in front.
  const prefix = kind && !name.toUpperCase().includes(kind[1].toUpperCase()) ? kind[1] + " " : "";
  $("warning-title").textContent = prefix + name;
  $("warning-detail").textContent = warningDetail(first);
  $("warning-more").textContent = warnings.length > 1 ? `+${warnings.length - 1} weitere` : "";
  banner.classList.toggle("alarm", first.level === "alarm");
  banner.classList.toggle("acknowledged", first.acknowledged);
  const button = $("warning-acknowledge");
  // Alarms cannot be silenced; Rust would ignore it anyway.
  button.hidden = first.level === "alarm" || first.acknowledged;
  button.dataset.index = first.index;
}

$("warning-acknowledge").addEventListener("click", () => {
  const index = Number($("warning-acknowledge").dataset.index);
  invoke("acknowledge_warning", { index }).catch((error) => console.warn(error));
});

let warningSettings = null;

const warningInputs = [
  ["set-warn-horizontal", "out-warn-horizontal", "horizontalM", (v) => distance(v)],
  ["set-warn-vertical", "out-warn-vertical", "verticalM", (v) => Math.round(v) + " m"],
  ["set-warn-lookahead", "out-warn-lookahead", "lookaheadS", (v) => Math.round(v) + " s"],
  ["set-warn-volume", "out-warn-volume", "volume", (v) => Math.round(v * 100) + " %"],
];

function showWarningSettings() {
  if (!warningSettings) return;
  for (const [input, output, field, format] of warningInputs) {
    $(input).value = warningSettings[field];
    $(output).textContent = format(warningSettings[field]);
  }
  const enabled = $("warning-enabled");
  enabled.classList.toggle("on", warningSettings.enabled);
  enabled.textContent = warningSettings.enabled ? "Warnung an" : "Warnung aus";
  $("warning-categories").replaceChildren(
    ...AIRSPACE_CATEGORIES.map(([category, label]) => {
      const button = document.createElement("button");
      button.className = "button";
      button.textContent = label;
      const on = warningSettings.categories.includes(category);
      button.classList.toggle("on", on);
      button.addEventListener("click", () => {
        const categories = on
          ? warningSettings.categories.filter((name) => name !== category)
          : warningSettings.categories.concat([category]);
        applyWarningSettings({ ...warningSettings, categories });
      });
      return button;
    }),
  );
}

async function applyWarningSettings(changed) {
  try {
    await invoke("set_warning_settings", { settings: changed });
    warningSettings = changed;
    $("warning-message").textContent = "";
  } catch (error) {
    $("warning-message").textContent = "Nicht übernommen: " + error;
  }
  showWarningSettings();
}

for (const [input, output, field, format] of warningInputs) {
  $(input).addEventListener("input", () => {
    $(output).textContent = format(Number($(input).value));
  });
  $(input).addEventListener("change", () => {
    applyWarningSettings({ ...warningSettings, [field]: Number($(input).value) });
  });
}

$("warning-enabled").addEventListener("click", () => {
  applyWarningSettings({ ...warningSettings, enabled: !warningSettings.enabled });
});

$("reset-warning").addEventListener("click", async () => {
  applyWarningSettings(await invoke("default_warning_settings"));
});

let recordingFile = null;

function renderRecording(recording) {
  const element = $("recording");
  recordingFile = running ? (recording?.fileName ?? null) : null;
  element.className = recording?.error ? "bad" : "";
  if (!running || !recording) {
    element.textContent = "";
  } else if (recording.error) {
    element.textContent = "Aufzeichnung: Speichern fehlgeschlagen – " + recording.error;
  } else if (recording.fixCount === 0) {
    element.textContent = "Aufzeichnung: warte auf GPS";
  } else {
    element.textContent = `● Aufzeichnung: ${duration(recording.fixCount)}`;
  }
}

// Seconds as "1:05 h" or "12:07 min".
function duration(seconds) {
  const two = (n) => String(n).padStart(2, "0");
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return h > 0 ? `${h}:${two(m)} h` : `${m}:${two(seconds % 60)} min`;
}

function renderSensors(status) {
  $("sensors").innerHTML = sensorRows
    .map(([label, key, limit]) => {
      const stats = status?.[key];
      if (!stats) return `<tr><td>${label}</td><td colspan="4">–</td></tr>`;
      const stale = stats.ageS == null || stats.ageS > limit;
      const gap = stats.longestGapS > limit;
      return `<tr>
        <td>${label}</td>
        <td>${stats.total}</td>
        <td>${stats.rateHz.toFixed(1)}</td>
        <td class="${stale ? "bad" : ""}">${number(stats.ageS, 1)}</td>
        <td class="${gap ? "bad" : ""}">${stats.longestGapS.toFixed(2)}</td>
      </tr>`;
    })
    .join("");

  if (status) {
    const log = $("log");
    const atBottom = log.scrollTop + log.clientHeight >= log.scrollHeight - 4;
    log.textContent = status.summaries.join("\n");
    if (atBottom) log.scrollTop = log.scrollHeight;
  }
}

// ---- Start / stop ----

function setRunning(value) {
  running = value;
  $("toggle").textContent = running ? "Stopp" : "Start";
  $("toggle").classList.toggle("primary", !running);
  $("tone-climb").disabled = !running;
  $("tone-sink").disabled = !running;
  $("alert-near").disabled = !running;
  $("alert-alarm").disabled = !running;
  if (!running) {
    testTone = null;
    showTestTone();
    testAlert = null;
    showTestAlert();
  }
}

async function toggle() {
  $("toggle").disabled = true;
  try {
    if (running) {
      await invoke("stop");
      setRunning(false);
      $("info").textContent = "gestoppt";
      if (activePage === "flights") loadFlights();
    } else {
      const info = await invoke("start");
      FlightMap.clearTrack();
      setRunning(true);
      $("interrupted").hidden = true;
      const s = info.sensors;
      $("info").textContent = s.native
        ? `Barometer: ${s.altimeter ? "ja" : "NEIN"} · Bewegung: ${s.motion ? "ja" : "NEIN"} · ` +
          `Standort: ${s.locationAuthorization}\nAudio-Session: ${s.audioSession} · Ton: ${info.audio}`
        : `Simulierter Flug (kein Gerät) · Ton: ${info.audio}`;
    }
  } catch (error) {
    $("info").textContent = "Fehler: " + error;
  }
  $("toggle").disabled = false;
}

$("toggle").addEventListener("click", toggle);

// ---- Permissions ----
//
// Without the right permissions iOS silently delivers nothing (or only rough
// positions), so problems are shown above the start button with a way to fix them.
// They are checked every few seconds and whenever the app comes back to the
// foreground, e.g. after the user changed them in the Settings app.

/** Problems with the given permissions: [text, isHint]. Empty if all is fine. */
function permissionProblems(p) {
  const problems = [];
  if (p.location === "denied" || p.location === "restricted") {
    problems.push([
      "Kein Standortzugriff: keine Position, keine Geschwindigkeit, keine Aufzeichnung. " +
        "Unter Standort „Immer“ wählen.",
      false,
    ]);
  } else if (p.location === "whenInUse") {
    problems.push([
      "Standort nur „Beim Verwenden der App“. Empfohlen ist „Immer“, damit die " +
        "Aufzeichnung bei gesperrtem Bildschirm sicher weiterläuft.",
      true,
    ]);
  }
  if (p.location !== "denied" && p.location !== "restricted" && !p.preciseLocation) {
    problems.push([
      "„Genauer Standort“ ist aus: Positionen sind nur auf einige Kilometer genau. " +
        "Unter Standort „Genauer Standort“ einschalten.",
      false,
    ]);
  }
  if (p.motion === "denied" || p.motion === "restricted") {
    problems.push([
      "„Bewegung & Fitness“ ist nicht erlaubt: ohne Barometer gibt es kein Vario. " +
        "Unter „Bewegung & Fitness“ einschalten.",
      false,
    ]);
  }
  return problems;
}

async function checkPermissions() {
  let problems = [];
  try {
    problems = permissionProblems(await invoke("permissions"));
  } catch (error) {
    console.warn("permissions", error);
  }
  const list = $("permission-problems");
  list.replaceChildren(
    ...problems.map(([text, isHint]) => {
      const item = document.createElement("li");
      item.textContent = text;
      if (isHint) item.className = "hint";
      return item;
    }),
  );
  $("permissions").hidden = problems.length === 0;
}

$("open-settings").addEventListener("click", () => {
  invoke("open_settings").catch((error) => console.warn("open_settings", error));
});
document.addEventListener("visibilitychange", () => {
  if (!document.hidden) checkPermissions();
});

// ---- Interrupted recording ----
//
// If the app ended without "Stop" (killed by iOS, crash, empty battery), the flight is
// saved up to the last few seconds. Say so once, so a cut-off flight is no surprise.

const timeFormat = new Intl.DateTimeFormat("de-DE", { hour: "2-digit", minute: "2-digit" });

async function checkInterrupted() {
  let session = null;
  try {
    session = await invoke("interrupted_session");
  } catch (error) {
    console.warn("interrupted_session", error);
  }
  if (!session) return;
  let text =
    "Die letzte Aufzeichnung wurde unerwartet beendet (z. B. von iOS beendet, " +
    "Absturz oder Akku leer). ";
  const summary = session.flight?.summary;
  if (summary?.startUtc) {
    const start = new Date(summary.startUtc);
    const end = new Date(start.getTime() + summary.durationS * 1000);
    text +=
      `Der Flug vom ${dateFormat.format(start)} ist bis ${timeFormat.format(end)} Uhr ` +
      "gespeichert und steht in der Flugliste.";
  } else if (session.flight) {
    text += `Der Flug ist gespeichert: ${session.flight.fileName}.`;
  } else {
    text += "Es wurde keine Position aufgezeichnet.";
  }
  $("interrupted-text").textContent = text;
  $("interrupted").hidden = false;
}

function hideInterrupted() {
  $("interrupted").hidden = true;
  invoke("dismiss_interrupted_session").catch((error) => console.warn(error));
}

$("dismiss-interrupted").addEventListener("click", hideInterrupted);

// ---- Recorded flights ----

const dateFormat = new Intl.DateTimeFormat("de-DE", {
  weekday: "short",
  day: "2-digit",
  month: "2-digit",
  year: "numeric",
  hour: "2-digit",
  minute: "2-digit",
});

function flightTitle(flight) {
  const start = flight.summary?.startUtc;
  // The browser converts the UTC time to local time.
  return start ? dateFormat.format(new Date(start)) : flight.fileName;
}

async function loadFlights() {
  let flights;
  try {
    flights = await invoke("list_flights");
  } catch (error) {
    $("flights-message").textContent = "Fehler: " + error;
    return;
  }
  $("flights-message").textContent = flights.length === 0 ? "Noch keine Flüge aufgezeichnet." : "";
  const list = $("flights");
  list.replaceChildren();
  for (const flight of flights) list.append(flightElement(flight));
}

// Built with DOM calls instead of innerHTML: file names are not ours to trust.
function flightElement(flight) {
  const element = document.createElement("div");
  element.className = "flight";

  const title = document.createElement("div");
  title.className = "flight-title";
  title.textContent = flightTitle(flight);

  const facts = document.createElement("div");
  facts.className = "flight-facts";
  const s = flight.summary;
  const active = flight.fileName === recordingFile;
  facts.textContent = s
    ? `${active ? "läuft · " : ""}${duration(s.durationS)} · max. ${s.maxAltitudeM} m · ` +
      `${(s.trackLengthM / 1000).toFixed(1)} km`
    : "Datei nicht lesbar";

  const buttons = document.createElement("div");
  buttons.className = "row";

  const show = document.createElement("button");
  show.className = "button";
  show.textContent = "Auf Karte";
  show.disabled = !s;
  show.addEventListener("click", () => showFlightOnMap(flight));

  // Deleting cannot be undone, so it takes two taps.
  const remove = document.createElement("button");
  remove.className = "button";
  remove.textContent = "Löschen";
  remove.disabled = active;
  let armed = false;
  remove.addEventListener("click", async () => {
    if (!armed) {
      armed = true;
      remove.textContent = "Wirklich löschen?";
      remove.classList.add("on");
      setTimeout(() => {
        armed = false;
        remove.textContent = "Löschen";
        remove.classList.remove("on");
      }, 3000);
      return;
    }
    try {
      await invoke("delete_flight", { fileName: flight.fileName });
    } catch (error) {
      $("flights-message").textContent = "Löschen fehlgeschlagen: " + error;
    }
    loadFlights();
  });

  buttons.append(show, remove);
  element.append(title, facts, buttons);
  return element;
}

async function showFlightOnMap(flight) {
  try {
    const points = await invoke("flight_track", { fileName: flight.fileName });
    FlightMap.showSavedFlight(points);
    $("saved-flight-label").textContent = flightTitle(flight);
    $("saved-flight").style.display = "flex";
    showPage("map");
  } catch (error) {
    $("flights-message").textContent = "Fehler: " + error;
  }
}

$("close-saved-flight").addEventListener("click", () => {
  FlightMap.hideSavedFlight();
  $("saved-flight").style.display = "none";
});

// ---- Settings ----

// [input id, output id, field of the settings object, how to show the value]
const settingInputs = [
  ["set-climb", "out-climb", "climbThresholdMps", (v) => signed(v) + " m/s"],
  ["set-sink", "out-sink", "sinkThresholdMps", (v) => signed(v) + " m/s"],
  ["set-volume", "out-volume", "volume", (v) => Math.round(v * 100) + " %"],
  ["set-damping", "out-damping", "dampingS", (v) => v.toFixed(1) + " s"],
];

function showSettings() {
  for (const [input, output, field, format] of settingInputs) {
    $(input).value = settings[field];
    $(output).textContent = format(settings[field]);
  }
}

async function applySettings(changed) {
  try {
    await invoke("set_settings", { settings: changed });
    settings = changed;
    $("settings-message").textContent = "";
  } catch (error) {
    // Rust rejected the values (or could not save them): show why.
    $("settings-message").textContent = "Nicht übernommen: " + error;
  }
  showSettings();
}

for (const [input, output, field, format] of settingInputs) {
  // "input" fires while dragging: only update the number. "change" fires on release:
  // apply and save.
  $(input).addEventListener("input", () => {
    $(output).textContent = format(Number($(input).value));
  });
  $(input).addEventListener("change", () => {
    applySettings({ ...settings, [field]: Number($(input).value) });
  });
}

$("reset-settings").addEventListener("click", async () => {
  applySettings(await invoke("default_settings"));
});

function showTestTone() {
  $("tone-climb").classList.toggle("on", testTone === 2.5);
  $("tone-sink").classList.toggle("on", testTone === -4);
}

async function toggleTone(climb) {
  testTone = testTone === climb ? null : climb;
  await invoke("test_tone", { climbRateMps: testTone });
  showTestTone();
}

function showTestAlert() {
  $("alert-near").classList.toggle("on", testAlert === "near");
  $("alert-alarm").classList.toggle("on", testAlert === "alarm");
}

async function toggleAlert(level) {
  testAlert = testAlert === level ? null : level;
  try {
    await invoke("test_alert", { level: testAlert });
  } catch (error) {
    console.warn("test_alert", error);
  }
  showTestAlert();
}

$("alert-near").addEventListener("click", () => toggleAlert("near"));
$("alert-alarm").addEventListener("click", () => toggleAlert("alarm"));
$("tone-climb").addEventListener("click", () => toggleTone(2.5));
$("tone-sink").addEventListener("click", () => toggleTone(-4));

// ---- Map ----

FlightMap.configure({
  onFollowChange: (follow) => $("follow").classList.toggle("on", follow),
  onMessage: (text) => {
    $("map-message").textContent = text;
    $("map-message").style.display = text ? "block" : "none";
  },
  onTap: showAirspacesAt,
});
$("follow").addEventListener("click", () => FlightMap.toggleFollow());

// Offline regions win over the online style. The style text is compared so the map is
// only reloaded when something changed.
let onlineStyleUrl = null;
let currentStyle = null;

function showOfflineStyle(variant) {
  for (const button of document.querySelectorAll("#offline-style button")) {
    button.classList.toggle("on", button.dataset.style === variant);
  }
}

for (const button of document.querySelectorAll("#offline-style button")) {
  button.addEventListener("click", async () => {
    try {
      await invoke("set_offline_map_style", { style: button.dataset.style });
      await updateMapStyle();
    } catch (error) {
      $("settings-message").textContent = "Nicht übernommen: " + error;
    }
  });
}

// ---- Light or dark ----

let mapLook = { dark: false, relief: false };

function showMapLook() {
  document.documentElement.dataset.theme = mapLook.dark ? "dark" : "light";
  for (const button of document.querySelectorAll("#map-look button")) {
    button.classList.toggle("on", (button.dataset.look === "dark") === mapLook.dark);
  }
  for (const button of document.querySelectorAll("#map-relief button")) {
    button.classList.toggle("on", (button.dataset.relief === "on") === mapLook.relief);
  }
  $("map-relief").hidden = !mapLook.dark;
  FlightMap.setLook(mapLook);
}

async function saveMapLook(next) {
  const previous = mapLook;
  try {
    await invoke("set_map_look", { look: next });
    mapLook = next;
    showMapLook();
    $("settings-message").textContent = "";
    if (next.dark !== previous.dark) {
      await updateMapStyle();
      // The airspace list shows the colours of the current look.
      loadAirspaces();
    }
  } catch (error) {
    $("settings-message").textContent = "Nicht übernommen: " + error;
  }
}

for (const button of document.querySelectorAll("#map-look button")) {
  button.addEventListener("click", () => {
    saveMapLook({ ...mapLook, dark: button.dataset.look === "dark" });
  });
}
for (const button of document.querySelectorAll("#map-relief button")) {
  button.addEventListener("click", () => {
    saveMapLook({ ...mapLook, relief: button.dataset.relief === "on" });
  });
}

async function updateMapStyle() {
  let style = onlineStyleUrl;
  try {
    const maps = await invoke("offline_maps");
    showOfflineStyle(maps.variant);
    if (maps.style) style = maps.style;
  } catch (error) {
    console.warn("offline_maps", error);
  }
  if (style === null || style === currentStyle) return;
  currentStyle = style;
  // The offline style comes as JSON text, the online one as a URL.
  FlightMap.setStyle(style.startsWith("{") ? JSON.parse(style) : style);
}

$("save-map-style").addEventListener("click", async () => {
  try {
    const url = await invoke("set_map_style", { url: $("set-map-style").value });
    $("set-map-style").value = url;
    onlineStyleUrl = url;
    await updateMapStyle();
    $("settings-message").textContent = "";
  } catch (error) {
    $("settings-message").textContent = "Nicht übernommen: " + error;
  }
});

// ---- Airspaces: OpenAir files in the folder "airspace" ----

// Categories as named in airspace.rs, in the order of the buttons.
const AIRSPACE_CATEGORIES = [
  ["ctr", "CTR"],
  ["controlled", "C / D"],
  ["classE", "E"],
  ["tmz", "TMZ"],
  ["rmz", "RMZ"],
  ["restricted", "Beschränkung (ED-R)"],
  ["prohibited", "Sperrgebiet"],
  ["danger", "Gefahr, Fallschirm"],
  ["gliderSector", "Segelflug"],
  ["reserved", "TRA (Österreich)"],
  ["other", "Sonstige (FIS, LFA)"],
];

let hiddenAirspace = [];

async function loadAirspaces() {
  try {
    showAirspaces(await invoke("airspaces"));
  } catch (error) {
    console.warn("airspaces", error);
    $("airspace-files").textContent = "Lufträume nicht lesbar: " + error;
  }
}

// ---- Airspaces from openAIP ----

let airspaceCountries = [];

async function loadAirspaceCountries() {
  try {
    airspaceCountries = await invoke("airspace_countries");
  } catch (error) {
    console.warn("airspace_countries", error);
  }
  showAirspaceCountries();
}

function showAirspaceCountries() {
  $("airspace-countries").replaceChildren(
    ...airspaceCountries.map((country) => {
      const button = document.createElement("button");
      button.className = "button";
      button.textContent = country.name;
      button.classList.toggle("on", country.selected);
      button.addEventListener("click", () => {
        country.selected = !country.selected;
        showAirspaceCountries();
      });
      return button;
    }),
  );
}

const UPDATE_OUTCOMES = { updated: "neu geladen", unchanged: "aktuell", failed: "fehlgeschlagen" };

$("update-airspaces").addEventListener("click", async () => {
  const button = $("update-airspaces");
  const countries = airspaceCountries.filter((c) => c.selected).map((c) => c.code);
  if (countries.length === 0) {
    $("airspace-update-message").textContent = "Kein Land gewählt.";
    return;
  }
  button.disabled = true;
  $("airspace-update-message").textContent = "Frage openAIP …";
  try {
    const update = await invoke("update_airspaces", { countries });
    showAirspaces(update.airspaces);
    const lines = update.results.map((r) => {
      let line = `${r.name}: ${UPDATE_OUTCOMES[r.outcome] ?? r.outcome}`;
      if (r.airspaces != null) line += ` (${r.airspaces} Lufträume)`;
      if (r.error) line += ` – ${r.error}`;
      return line;
    });
    // The same country as an old OpenAir file and as an openAIP export would warn twice.
    const names = update.airspaces.files.map((f) => f.name);
    const doubled = countries.filter(
      (code) => names.includes(code + "_asp.geojson") && names.some((n) => n.startsWith(code + "_asp") && n.endsWith(".txt")),
    );
    if (doubled.length > 0) {
      lines.push("Doppelt vorhanden (bitte die .txt-Datei löschen): " + doubled.join(", ").toUpperCase());
    }
    $("airspace-update-message").textContent = lines.join(" · ");
  } catch (error) {
    $("airspace-update-message").textContent = "Aktualisieren fehlgeschlagen: " + error;
  }
  button.disabled = false;
});

function showAirspaces(result) {
  hiddenAirspace = result.hidden;
  FlightMap.setAirspaceHidden(hiddenAirspace);
  FlightMap.setAirspaces(result.geojson);
  showAirspaceFiles(result.files);
  showAirspaceCategories();
}

// ---- Tapping the map lists the airspaces at that place ----

let airspaceRequest = 0; // answers to older taps are dropped

async function showAirspacesAt(latitude, longitude) {
  const request = ++airspaceRequest;
  let found;
  try {
    found = await invoke("airspaces_at", { latitude, longitude });
  } catch (error) {
    console.warn("airspaces_at", error);
    return;
  }
  if (request !== airspaceRequest) return;
  // Only what the map shows.
  found = found.filter((airspace) => !hiddenAirspace.includes(airspace.category));
  FlightMap.markTap(latitude, longitude);
  const list = $("airspace-info-list");
  if (found.length === 0) {
    list.textContent = "Hier ist kein angezeigter Luftraum.";
  } else {
    list.replaceChildren(...found.map(airspaceEntry));
  }
  $("airspace-info").hidden = false;
}

function airspaceEntry(airspace) {
  const element = document.createElement("div");
  element.className = "airspace-entry";
  element.style.borderColor = FlightMap.airspaceColor(airspace.category);
  const kind = AIRSPACE_CATEGORIES.find(([category]) => category === airspace.category);
  const parts = [kind ? kind[1] : airspace.category];
  if (airspace.class && airspace.class !== "UNC") parts.push("Klasse " + airspace.class);
  const lines = [parts.join(" · "), airspace.lower + " bis " + airspace.upper];
  const radio = [airspace.frequency, airspace.station].filter(Boolean).join(" · ");
  if (radio) lines.push(radio);
  const name = document.createElement("div");
  name.textContent = airspace.name || "(ohne Namen)";
  element.append(
    name,
    ...lines.map((line) => {
      const small = document.createElement("small");
      small.textContent = line;
      return small;
    }),
  );
  return element;
}

$("close-airspace-info").addEventListener("click", () => {
  airspaceRequest++;
  $("airspace-info").hidden = true;
  FlightMap.markTap(null);
});

function showAirspaceFiles(files) {
  $("airspace-none").hidden = files.length > 0;
  $("airspace-files").replaceChildren(
    ...files.map((file) => {
      const element = document.createElement("div");
      element.className = "flight";
      const title = document.createElement("div");
      title.className = "flight-title";
      title.textContent = file.name;
      const facts = document.createElement("div");
      facts.className = "flight-facts";
      if (file.error) {
        facts.textContent = "Nicht lesbar: " + file.error;
      } else {
        const parts = [file.airspaces + " Lufträume"];
        if (file.issueCount > 0) parts.push(file.issueCount + " übersprungen oder unklar");
        facts.textContent = parts.join(" · ");
      }
      element.append(title, facts);
      element.append(deleteAirspaceButton(file.name));
      if (file.issues.length > 0) {
        // The details are in English (from the parser); they are meant for checking a file.
        const details = document.createElement("details");
        const summary = document.createElement("summary");
        summary.textContent = "Meldungen";
        const list = document.createElement("pre");
        list.className = "issues";
        list.textContent = file.issues.join("\n");
        details.append(summary, list);
        element.append(details);
      }
      return element;
    }),
  );
}

// Deleting cannot be undone, so it takes two taps (as for flights).
function deleteAirspaceButton(name) {
  const row = document.createElement("div");
  row.className = "row";
  const button = document.createElement("button");
  button.className = "button";
  button.textContent = "Löschen";
  let armed = false;
  button.addEventListener("click", async () => {
    if (!armed) {
      armed = true;
      button.textContent = "Wirklich löschen?";
      button.classList.add("on");
      setTimeout(() => {
        armed = false;
        button.textContent = "Löschen";
        button.classList.remove("on");
      }, 3000);
      return;
    }
    try {
      showAirspaces(await invoke("delete_airspace_file", { name }));
    } catch (error) {
      $("airspace-files").prepend("Löschen fehlgeschlagen: " + error);
    }
  });
  row.append(button);
  return row;
}

function showAirspaceCategories() {
  $("airspace-categories").replaceChildren(
    ...AIRSPACE_CATEGORIES.map(([category, label]) => {
      const button = document.createElement("button");
      button.className = "button";
      button.textContent = label;
      button.classList.toggle("on", !hiddenAirspace.includes(category));
      button.addEventListener("click", async () => {
        const hidden = hiddenAirspace.includes(category)
          ? hiddenAirspace.filter((name) => name !== category)
          : hiddenAirspace.concat([category]);
        try {
          hiddenAirspace = await invoke("set_hidden_airspace", { hidden });
          FlightMap.setAirspaceHidden(hiddenAirspace);
          showAirspaceCategories();
        } catch (error) {
          $("settings-message").textContent = "Nicht übernommen: " + error;
        }
      });
      return button;
    }),
  );
}

// ---- Regions: installed and on offer from ZeroGrid Base ----
//
// The list comes from Rust in two steps: at once with only the installed regions, then
// with the desktop's catalog, which can take up to 20 s if the desktop is not reachable.
// Downloads run in Rust; their progress comes as the event "region-download".

let regionList = { source: "", catalogError: null, regions: [] };
let regionRequest = 0; // answers to older requests are dropped
let catalogShown = false; // the list already holds what the desktop offers
const downloadProgress = new Map(); // region id -> { done, total }
const progressViews = new Map(); // region id -> { bar, text } of the shown list

const megabytes = (bytes) => `${Math.round(bytes / 1e6)} MB`;

// Versions are build times like "202611181120".
function versionDate(version) {
  const m = /^(\d{4})(\d{2})(\d{2})/.exec(version ?? "");
  return m ? `${m[3]}.${m[2]}.${m[1]}` : version;
}

// `withCatalog: false` reads only the installed regions and never the network.
async function refreshRegions(withCatalog = true) {
  const request = ++regionRequest;
  // Without the catalog only the first time: later the list would shrink for a moment.
  for (const catalog of catalogShown ? [true] : [false, true]) {
    if (catalog && (!withCatalog || !regionList.source)) break;
    if (catalog) $("regions-message").textContent = "Suche den Desktop …";
    try {
      const list = await invoke("map_regions", { catalog });
      if (request !== regionRequest) return;
      regionList = list;
      if (catalog) catalogShown = !list.catalogError;
      showRegionList();
    } catch (error) {
      if (request !== regionRequest) return;
      $("regions-message").textContent = "Fehler: " + error;
      return;
    }
  }
}

function showRegionList() {
  const { regions, source, catalogError } = regionList;
  progressViews.clear();
  $("regions").replaceChildren(...regions.map(regionElement));
  $("regions-none").hidden = regions.length > 0;
  let message = "";
  if (catalogError) {
    message = "Desktop nicht erreichbar. Läuft die Freigabe in ZeroGrid Base? (" + catalogError + ")";
  } else if (source && !regions.some((region) => region.state !== "installed")) {
    message = "Verbunden. Der Desktop bietet keine neueren Regionen an.";
  }
  $("regions-message").textContent = message;
}

const STATE_TEXT = {
  available: "auf dem Desktop",
  installed: "installiert",
  updateAvailable: "Update auf dem Desktop",
  downloading: "wird geladen",
};

// Built with DOM calls instead of innerHTML: names come from the network.
function regionElement(region) {
  const element = document.createElement("div");
  element.className = "flight region";

  const title = document.createElement("div");
  title.className = "flight-title";
  title.textContent = region.name;

  const facts = document.createElement("div");
  facts.className = "flight-facts";
  const parts = [megabytes(region.sizeBytes), STATE_TEXT[region.state]];
  if (region.installedVersion) parts.push("Stand " + versionDate(region.installedVersion));
  if (region.state === "updateAvailable") parts.push("neu: " + versionDate(region.version));
  facts.textContent = parts.join(" · ");
  element.append(title, facts);

  if (region.state === "downloading") {
    const bar = document.createElement("progress");
    bar.max = 1;
    const text = document.createElement("div");
    text.className = "flight-facts";
    progressViews.set(region.id, { bar, text });
    element.append(bar, text);
    showProgress(region.id);
  }

  const buttons = document.createElement("div");
  buttons.className = "row";
  if (region.state === "downloading") {
    buttons.append(regionButton("Abbrechen", () => invoke("cancel_region_download", { id: region.id })));
  } else {
    if (region.state === "available" || region.state === "updateAvailable") {
      const label = region.state === "available" ? "Laden" : "Aktualisieren";
      buttons.append(regionButton(label, () => startDownload(region)));
    }
    if (region.installedVersion) buttons.append(deleteButton(region));
  }
  element.append(buttons);
  return element;
}

function regionButton(label, action) {
  const button = document.createElement("button");
  button.className = "button";
  button.textContent = label;
  button.addEventListener("click", async () => {
    try {
      await action();
    } catch (error) {
      $("regions-message").textContent = "Fehler: " + error;
    }
  });
  return button;
}

// Deleting cannot be undone, so it takes two taps (as for flights).
function deleteButton(region) {
  const button = document.createElement("button");
  button.className = "button";
  button.textContent = "Löschen";
  let armed = false;
  button.addEventListener("click", async () => {
    if (!armed) {
      armed = true;
      button.textContent = "Wirklich löschen?";
      button.classList.add("on");
      setTimeout(() => {
        armed = false;
        button.textContent = "Löschen";
        button.classList.remove("on");
      }, 3000);
      return;
    }
    try {
      await invoke("delete_region", { id: region.id });
    } catch (error) {
      $("regions-message").textContent = "Löschen fehlgeschlagen: " + error;
    }
    await updateMapStyle();
    refreshRegions();
  });
  return button;
}

async function startDownload(region) {
  await invoke("download_region", { id: region.id });
  downloadProgress.set(region.id, { done: 0, total: region.sizeBytes });
  region.state = "downloading";
  showRegionList();
}

function showProgress(id) {
  const view = progressViews.get(id);
  const progress = downloadProgress.get(id);
  if (!view || !progress) return;
  const share = progress.total > 0 ? progress.done / progress.total : 0;
  view.bar.value = share;
  view.text.textContent =
    `${Math.floor(share * 100)} % · ${megabytes(progress.done)} von ${megabytes(progress.total)}`;
}

const DOWNLOAD_ERRORS = {
  region_download: "Verbindung zum Desktop unterbrochen. Nochmal laden setzt den Download fort.",
  region_checksum: "Eine Datei kam beschädigt an und wurde verworfen. Bitte nochmal laden.",
  region_io: "Konnte nicht gespeichert werden (Speicher voll?).",
};

window.__TAURI__.event.listen("region-download", async ({ payload }) => {
  const { region, done, total, state } = payload;
  if (state === "running") {
    downloadProgress.set(region, { done, total });
    if (progressViews.has(region)) {
      showProgress(region);
    } else {
      refreshRegions();
    }
    return;
  }
  downloadProgress.delete(region);
  const name = regionList.regions.find((r) => r.id === region)?.name ?? region;
  if (state === "done") {
    // The new region shows on the map at once.
    await updateMapStyle();
  }
  await refreshRegions();
  if (state === "done") {
    $("regions-message").textContent = `${name} ist geladen.`;
  } else if (state === "cancelled") {
    $("regions-message").textContent = `Laden von ${name} abgebrochen; es geht beim nächsten Laden weiter.`;
  } else {
    $("regions-message").textContent =
      `${name}: ${DOWNLOAD_ERRORS[payload.errorCode] ?? payload.error}`;
  }
});

async function connectMapSource() {
  try {
    const source = await invoke("set_map_source", { source: $("set-map-source").value });
    $("set-map-source").value = source;
    regionList.source = source;
    catalogShown = false;
    refreshRegions();
  } catch (error) {
    $("regions-message").textContent = "Nicht übernommen: " + error;
  }
}

$("connect-map-source").addEventListener("click", connectMapSource);

$("scan-map-source").addEventListener("click", async () => {
  $("scan-map-source").disabled = true;
  try {
    const scan = await invoke("scan_map_source");
    if (scan.address) {
      $("set-map-source").value = scan.address;
      await connectMapSource();
    } else if (scan.message) {
      $("regions-message").textContent = scan.message;
    }
  } catch (error) {
    $("regions-message").textContent = "Scannen fehlgeschlagen: " + error;
  } finally {
    $("scan-map-source").disabled = false;
  }
});

/** The scanner is a native part of the iPhone app; elsewhere the button stays hidden. */
async function showQrScanner() {
  try {
    $("scan-map-source").hidden = !(await invoke("permissions")).native;
  } catch (error) {
    console.warn("permissions", error);
  }
}

// ---- Polling ----
//
// The web view is paused while the screen is locked; that is fine, because everything
// important runs in Rust. After unlocking, polling simply resumes and the track
// request below fetches all points recorded in the meantime.

async function pollStatus() {
  try {
    const status = await invoke("status");
    if (status && !running) setRunning(true);
    render(status);
  } catch (error) {
    $("info").textContent = "Fehler: " + error;
  }
}

async function pollTrack() {
  if (!running) return;
  try {
    const points = await invoke("track", { from: FlightMap.trackLength() });
    if (points.length > 0) FlightMap.addTrackPoints(points);
  } catch (error) {
    console.warn("track", error);
  }
}

async function init() {
  render(null);
  settings = await invoke("get_settings");
  showSettings();
  warningSettings = await invoke("get_warning_settings");
  showWarningSettings();
  onlineStyleUrl = await invoke("get_map_style");
  $("set-map-style").value = onlineStyleUrl;
  $("set-map-source").value = await invoke("get_map_source");
  mapLook = await invoke("get_map_look");
  showMapLook();
  showQrScanner();
  // The desktop is asked only when the settings are opened: on iOS the first
  // connection asks for the local network permission.
  await updateMapStyle();
  refreshRegions(false);
  loadAirspaces();
  loadAirspaceCountries();
  checkPermissions();
  checkInterrupted();
  // Also catches the answers to the permission dialogs iOS shows on the first start.
  setInterval(checkPermissions, 5000);
  setInterval(pollStatus, 200);
  setInterval(pollTrack, 1000);
}

init().catch((error) => {
  $("info").textContent = "Fehler beim Start: " + error;
});
