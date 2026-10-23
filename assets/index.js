/**
 * @typedef {object} PathItem
 * @property {"Dir"|"SymlinkDir"|"File"|"SymlinkFile"} path_type
 * @property {string} name
 * @property {number} mtime
 * @property {number} size
 * @property {boolean} allow_upload
 * @property {boolean} allow_delete
 * @property {boolean} readonly_descendant
 */

/**
 * @typedef {object} DATA
 * @property {string} href
 * @property {string} uri_prefix
 * @property {"Index" | "Edit" | "View"} kind
 * @property {PathItem[]} paths
 * @property {boolean} allow_upload
 * @property {boolean} allow_delete
 * @property {boolean} allow_search
 * @property {boolean} allow_archive
 * @property {boolean} auth
 * @property {string} user
 * @property {boolean} dir_exists
 * @property {string} editable
 */

var DUFS_MAX_UPLOADINGS = 1;

/**
 * @type {DATA} DATA
 */
var DATA;

/**
 * @type {PARAMS}
 * @typedef {object} PARAMS
 * @property {string} q
 * @property {string} sort
 * @property {string} order
 */
const PARAMS = Object.fromEntries(new URLSearchParams(window.location.search).entries());

const IFRAME_FORMATS = [
  ".pdf",
  ".jpg", ".jpeg", ".png", ".gif", ".bmp", ".svg",
  ".mp4", ".mov", ".avi", ".wmv", ".flv", ".webm",
  ".mp3", ".ogg", ".wav", ".m4a",
];

const MAX_SUBPATHS_COUNT = 1000;

const ICONS = {
  dir: `<svg height="16" viewBox="0 0 14 16" width="14"><path fill-rule="evenodd" d="M13 4H7V3c0-.66-.31-1-1-1H1c-.55 0-1 .45-1 1v10c0 .55.45 1 1 1h12c.55 0 1-.45 1-1V5c0-.55-.45-1-1-1zM6 4H1V3h5v1z"></path></svg>`,
  symlinkFile: `<svg height="16" viewBox="0 0 12 16" width="12"><path fill-rule="evenodd" d="M8.5 1H1c-.55 0-1 .45-1 1v12c0 .55.45 1 1 1h10c.55 0 1-.45 1-1V4.5L8.5 1zM11 14H1V2h7l3 3v9zM6 4.5l4 3-4 3v-2c-.98-.02-1.84.22-2.55.7-.71.48-1.19 1.25-1.45 2.3.02-1.64.39-2.88 1.13-3.73.73-.84 1.69-1.27 2.88-1.27v-2H6z"></path></svg>`,
  symlinkDir: `<svg height="16" viewBox="0 0 14 16" width="14"><path fill-rule="evenodd" d="M13 4H7V3c0-.66-.31-1-1-1H1c-.55 0-1 .45-1 1v10c0 .55.45 1 1 1h12c.55 0 1-.45 1-1V5c0-.55-.45-1-1-1zM1 3h5v1H1V3zm6 9v-2c-.98-.02-1.84.22-2.55.7-.71.48-1.19 1.25-1.45 2.3.02-1.64.39-2.88 1.13-3.73C4.86 8.43 5.82 8 7.01 8V6l4 3-4 3H7z"></path></svg>`,
  file: `<svg height="16" viewBox="0 0 12 16" width="12"><path fill-rule="evenodd" d="M6 5H2V4h4v1zM2 8h7V7H2v1zm0 2h7V9H2v1zm0 2h7v-1H2v1zm10-7.5V14c0 .55-.45 1-1 1H1c-.55 0-1-.45-1-1V2c0-.55.45-1 1-1h7.5L12 4.5zM11 5L8 2H1v12h10V5z"></path></svg>`,
  download: `<svg width="16" height="16" viewBox="0 0 16 16"><path d="M.5 9.9a.5.5 0 0 1 .5.5v2.5a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-2.5a.5.5 0 0 1 1 0v2.5a2 2 0 0 1-2 2H2a2 2 0 0 1-2-2v-2.5a.5.5 0 0 1 .5-.5z"/><path d="M7.646 11.854a.5.5 0 0 0 .708 0l3-3a.5.5 0 0 0-.708-.708L8.5 10.293V1.5a.5.5 0 0 0-1 0v8.793L5.354 8.146a.5.5 0 1 0-.708.708l3 3z"/></svg>`,
  move: `<svg width="16" height="16" viewBox="0 0 16 16"><path fill-rule="evenodd" d="M1.5 1.5A.5.5 0 0 0 1 2v4.8a2.5 2.5 0 0 0 2.5 2.5h9.793l-3.347 3.346a.5.5 0 0 0 .708.708l4.2-4.2a.5.5 0 0 0 0-.708l-4-4a.5.5 0 0 0-.708.708L13.293 8.3H3.5A1.5 1.5 0 0 1 2 6.8V2a.5.5 0 0 0-.5-.5z"/></svg>`,
  edit: `<svg width="16" height="16" viewBox="0 0 16 16"><path d="M12.146.146a.5.5 0 0 1 .708 0l3 3a.5.5 0 0 1 0 .708l-10 10a.5.5 0 0 1-.168.11l-5 2a.5.5 0 0 1-.65-.65l2-5a.5.5 0 0 1 .11-.168l10-10zM11.207 2.5 13.5 4.793 14.793 3.5 12.5 1.207 11.207 2.5zm1.586 3L10.5 3.207 4 9.707V10h.5a.5.5 0 0 1 .5.5v.5h.5a.5.5 0 0 1 .5.5v.5h.293l6.5-6.5zm-9.761 5.175-.106.106-1.528 3.821 3.821-1.528.106-.106A.5.5 0 0 1 5 12.5V12h-.5a.5.5 0 0 1-.5-.5V11h-.5a.5.5 0 0 1-.468-.325z"/></svg>`,
  delete: `<svg width="16" height="16" viewBox="0 0 16 16"><path d="M6.854 7.146a.5.5 0 1 0-.708.708L7.293 9l-1.147 1.146a.5.5 0 0 0 .708.708L8 9.707l1.146 1.147a.5.5 0 0 0 .708-.708L8.707 9l1.147-1.146a.5.5 0 0 0-.708-.708L8 8.293 6.854 7.146z"/><path d="M14 14V4.5L9.5 0H4a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2zM9.5 3A1.5 1.5 0 0 0 11 4.5h2V14a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V2a1 1 0 0 1 1-1h5.5v2z"/></svg>`,
  view: `<svg width="16" height="16" viewBox="0 0 16 16"><path d="M4 0a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V2a2 2 0 0 0-2-2zm0 1h8a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V2a1 1 0 0 1 1-1"/></svg>`,
}

/**
 * Live region announcing finished and failed uploads.
 * @type Element
 */
let $uploadAnnouncer;

/**
 * Announce a message to screen readers. Each message is its own node, so
 * several uploads finishing together are all read out, each exactly once.
 * @param {string} message
 */
function announce(message) {
  const $message = document.createElement("div");
  $message.textContent = message;
  $uploadAnnouncer.append($message);
  while ($uploadAnnouncer.childElementCount > 20) {
    $uploadAnnouncer.firstElementChild.remove();
  }
}

/**
 * Names of the selected entries, as listed in DATA.paths.
 * @type Set<string>
 */
const selection = new Set();
/**
 * The row a shift-click range starts from: the last one clicked.
 * @type string | null
 */
let selectAnchor = null;
/**
 * Whether a bulk action is running.
 */
let bulkBusy = false;

// The browser needs a breather between two programmatic downloads, or it
// drops all but the first.
const DOWNLOAD_GAP_MS = 300;
// More downloads than this are confirmed first.
const DOWNLOAD_CONFIRM_ABOVE = 10;
// A delete confirmation lists this many names, then "and N more".
const CONFIRM_LIST_MAX = 10;

/**
 * @type Element
 */
let $pathsTable;
/**
 * @type Element
 */
let $pathsTableHead;
/**
 * @type Element
 */
let $pathsTableBody;
/**
 * @type Element
 */
let $uploadersTable;
/**
 * @type Element
 */
let $emptyFolder;
/**
 * @type Element
 */
let $filterStatus;
/**
 * What the search field has typed so far; rows whose name lacks it are hidden.
 */
let filterText = "";
/**
 * @type Element
 */
let $editor;
/**
 * @type Element
 */
let $loginBtn;
/**
 * @type Element
 */
let $logoutBtn;
/**
 * @type Element
 */
let $userName;
/**
 * @type Element
 */
let $saveBtn;
/**
 * @type Element
 */
let $overwriteBtn;
/**
 * @type Element
 */
let $saveStatus;
let saveStatusTimer;
/**
 * ETag of the file as last loaded or saved by the editor, sent as If-Match.
 * @type {string|null}
 */
let editorEtag = null;
/**
 * Editor content as last loaded or saved; the editor is dirty when it differs.
 * @type {string}
 */
let editorSavedValue = "";
/**
 * Whether Tab inserts a tab character instead of moving focus.
 */
let editorTabTrap = true;

// manage unload event to prevent leaving with uploads in progress or unsaved edits
const beforeUnloadHandler = (event) => {
  if (Uploader.queues.length > 0 || Uploader.runnings > 0 || isEditorDirty()) {
    event.preventDefault();
    event.returnValue = '';
    return ''; // for some browsers
  }
};

async function initialize() {
  const $indexData = document.getElementById('index-data');
  if (!$indexData) {
    showNotice("No data");
    return;
  }

  DATA = JSON.parse(decodeBase64($indexData.innerHTML));

  await ready();
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", initialize, {
    once: true,
  });
} else {
  queueMicrotask(initialize);
}

async function ready() {
  $pathsTable = document.querySelector(".paths-table");
  $pathsTableHead = document.querySelector(".paths-table thead");
  $pathsTableBody = document.querySelector(".paths-table tbody");
  $uploadersTable = document.querySelector(".uploaders-table");
  $uploadAnnouncer = document.querySelector(".upload-announcer");
  $emptyFolder = document.querySelector(".empty-folder");
  $filterStatus = document.querySelector(".filter-status");
  $editor = document.querySelector(".editor");
  $loginBtn = document.querySelector(".login-btn");
  $logoutBtn = document.querySelector(".logout-btn");
  $userName = document.querySelector(".user-name");
  $saveStatus = document.querySelector(".save-status");

  window.addEventListener('beforeunload', beforeUnloadHandler);

  addBreadcrumb(DATA.href, DATA.uri_prefix);

  if (DATA.kind === "Index") {
    document.title = `Index of ${DATA.href} - Dufs`;
    document.querySelector(".index-page").classList.remove("hidden");

    await setupIndexPage();
  } else if (DATA.kind === "Edit") {
    document.title = `Edit ${DATA.href} - Dufs`;
    document.querySelector(".editor-page").classList.remove("hidden");

    await setupEditorPage();
  } else if (DATA.kind === "View") {
    document.title = `View ${DATA.href} - Dufs`;
    document.querySelector(".editor-page").classList.remove("hidden");

    await setupEditorPage();
  }
}

class Uploader {
  /**
   *
   * @param {File} file
   * @param {string[]} pathParts
   */
  constructor(file, pathParts) {
    /**
     * @type Element
     */
    this.$status = null;
    /**
     * @type Element
     */
    this.$size = null;
    /**
     * @type Element
     */
    this.$percent = null;
    /**
     * @type Element
     */
    this.$speed = null;
    /**
     * @type Element
     */
    this.$eta = null;
    // speed ring buffer, mirrors curl's progress meter (6 records, one per
    // second, giving a 5-second window) so the displayed speed stays stable
    this.startTime = 0;
    this.speedRecords = 0;
    this.speedAmounts = [];
    this.speedTimes = [];
    this.currentSpeed = 0;
    this.uploadOffset = 0;
    this.lastUptime = 0;
    this.name = [...pathParts, file.name].join("/");
    this.idx = Uploader.globalIdx++;
    this.file = file;
    this.url = newUrl(this.name);
  }

  /**
   * Insert this upload's row into the uploaders table and bind its cells.
   */
  render() {
    const { idx, name, url } = this;
    const escapedName = escapeHtml(name);
    $uploadersTable.insertAdjacentHTML("beforeend", `
  <tr id="upload${idx}" class="uploader">
    <td class="path cell-icon">
      ${getPathSvg()}
    </td>
    <td class="path cell-name">
      <a href="${escapeHtml(url)}">${escapedName}</a>
    </td>
    <td class="cell-status" id="uploadStatus${idx}"></td>
    <td class="cell-size" id="uploadSize${idx}"></td>
    <td class="cell-percent" id="uploadPercent${idx}"></td>
    <td class="cell-speed" id="uploadSpeed${idx}"></td>
    <td class="cell-eta" id="uploadEta${idx}"></td>
  </tr>`);
    $uploadersTable.classList.remove("hidden");
    $emptyFolder.classList.add("hidden");
    this.$status = document.getElementById(`uploadStatus${idx}`);
    this.$percent = document.getElementById(`uploadPercent${idx}`);
    this.$size = document.getElementById(`uploadSize${idx}`);
    this.$speed = document.getElementById(`uploadSpeed${idx}`);
    this.$eta = document.getElementById(`uploadEta${idx}`);
    this.$name = document.querySelector(`#upload${idx} .cell-name a`);
    this.setStatus("-", "Queued");
    this.$percent.textContent = formatPercent(0);
    this.$size.textContent = formatUploadSize(0, this.file.size);
  }

  /**
   * Show a status glyph with a text alternative: the glyph is hidden from
   * assistive technology and a visually hidden word is read instead.
   * @param {string} glyph
   * @param {string} label
   * @param {Node[]} extra further nodes shown after the status
   */
  setStatus(glyph, label, ...extra) {
    const $glyph = document.createElement("span");
    $glyph.setAttribute("aria-hidden", "true");
    $glyph.setAttribute("title", label);
    $glyph.textContent = glyph;
    const $label = document.createElement("span");
    $label.className = "sr-only";
    $label.textContent = label;
    this.$status.replaceChildren($glyph, $label, ...extra);
  }

  /**
   * Mark the row failed and show why as visible text.
   * @param {string} reason
   * @param {Node[]} extra further nodes shown after the reason
   */
  setFailed(reason, ...extra) {
    const $reason = document.createElement("span");
    $reason.className = "status-reason";
    $reason.textContent = reason;
    this.setStatus("✗", "Failed", $reason, ...extra);
    announce(`Upload failed: ${this.name}: ${reason}`);
  }

  upload() {
    this.render();
    Uploader.queues.push(this);
    Uploader.runQueue();
  }

  /**
   * Show a failed row for an entry that could not be read, so it never
   * enters the upload queue and offers no retry.
   * @param {string} name entry name
   * @param {string[]} pathParts parent directories
   * @param {string} reason
   */
  static unreadable(name, pathParts, reason) {
    const uploader = new Uploader(new File([], name), pathParts);
    uploader.render();
    uploader.setFailed(reason);
  }

  ajax() {
    const { url } = this;

    this.startTime = Date.now();
    this.lastUptime = this.startTime;
    this.speedRecords = 1;
    this.speedAmounts = [0];
    this.speedTimes = [0];
    this.currentSpeed = 0;
    this.setStatus("↑", "Uploading");

    const ajax = new XMLHttpRequest();
    ajax.upload.addEventListener("progress", e => this.progress(e), false);
    ajax.addEventListener("readystatechange", () => {
      if (ajax.readyState === 4) {
        if (ajax.status >= 200 && ajax.status < 300) {
          this.complete();
        } else {
          if (ajax.status != 0) {
            const code = ajax.getResponseHeader("x-dufs-error-code");
            if (code === "upload-disabled") {
              this.fail("upload is disabled");
            } else if (code === "delete-disabled") {
              this.fail("overwriting is disabled");
            } else if (code === "readonly-descendant") {
              this.fail("directory contains read-only entries");
            } else {
              this.fail(`${ajax.status} ${ajax.statusText}`);
            }
          }
        }
      }
    })
    ajax.addEventListener("error", () => this.fail("network error"), false);
    ajax.addEventListener("abort", () => this.fail("upload aborted"), false);
    if (this.uploadOffset > 0) {
      ajax.open("PATCH", url);
      ajax.setRequestHeader("X-Update-Range", "append");
      ajax.send(this.file.slice(this.uploadOffset));
    } else {
      ajax.open("PUT", url);
      ajax.send(this.file);
      // setTimeout(() => ajax.abort(), 3000);
    }
  }

  async retry() {
    const { url } = this;
    this.setStatus("↑", "Uploading");
    let res = await fetch(url, {
      method: "HEAD",
    });
    let uploadOffset = 0;
    if (res.status == 200) {
      let value = res.headers.get("content-length");
      uploadOffset = parseInt(value) || 0;
    }
    this.uploadOffset = uploadOffset;
    Uploader.runnings++;
    this.ajax();
  }

  progress(event) {
    const now = Date.now();
    this.calcSpeed(now - this.startTime, event.loaded);
    if (now - this.lastUptime < 300) return; // throttle update for safari
    this.lastUptime = now;
    const uploadedSize = event.loaded + this.uploadOffset;
    const [speedValue, speedUnit] = formatFileSize(this.currentSpeed);
    this.$percent.textContent = formatPercent((uploadedSize / this.file.size) * 100);
    this.$size.textContent = formatUploadSize(uploadedSize, this.file.size);
    this.$speed.textContent = `${speedValue} ${speedUnit}/s`;
    this.$eta.textContent = formatDuration(this.currentSpeed > 0 ? (event.total - event.loaded) / this.currentSpeed : Infinity);
  }

  /**
   * Update the smoothed speed using curl's progress meter algorithm: keep a
   * 6-record ring buffer (one record per second, a 5-second window), add a
   * record at most once per second, and average over the buffered window.
   * @param {number} elapsed milliseconds since the transfer started
   * @param {number} amount bytes transferred so far
   */
  calcSpeed(elapsed, amount) {
    const records = 6;
    const iNext = this.speedRecords % records;
    let iLatest = iNext > 0 ? iNext - 1 : records - 1;
    if (elapsed - this.speedTimes[iLatest] < 1000) return;
    this.speedRecords++;
    iLatest = iNext;
    this.speedAmounts[iLatest] = amount;
    this.speedTimes[iLatest] = elapsed;
    const iOldest = this.speedRecords < records ? 0 : (iLatest + 1) % records;
    const duration = this.speedTimes[iLatest] - this.speedTimes[iOldest];
    const delta = this.speedAmounts[iLatest] - this.speedAmounts[iOldest];
    this.currentSpeed = duration > 0 ? delta / duration * 1000 : 0;
  }

  complete() {
    this.setStatus("✓", "Done");
    this.$percent.textContent = formatPercent(100);
    this.$size.textContent = formatUploadSize(this.file.size, this.file.size);
    this.$speed.textContent = "";
    this.$eta.textContent = "";
    announce(`Uploaded ${this.name}`);
    Uploader.runnings--;
    Uploader.runQueue();
  }

  /**
   * @param {string} reason
   */
  fail(reason) {
    const $retry = document.createElement("button");
    $retry.type = "button";
    $retry.className = "retry-btn";
    $retry.setAttribute("aria-label", `Retry upload ${this.name}`);
    $retry.setAttribute("title", "Retry");
    $retry.textContent = "↻";
    $retry.addEventListener("click", () => {
      // the button is replaced by the status, so keep keyboard focus in the row
      this.$name.focus();
      this.retry();
    });
    this.setFailed(reason, $retry);
    this.$speed.textContent = "";
    this.$eta.textContent = "";
    Uploader.runnings--;
    Uploader.runQueue();
  }
}

Uploader.globalIdx = 0;

Uploader.runnings = 0;

Uploader.auth = false;

/**
 * @type Uploader[]
 */
Uploader.queues = [];


Uploader.runQueue = async () => {
  if (Uploader.runnings >= DUFS_MAX_UPLOADINGS) return;
  if (Uploader.queues.length == 0) {
    // Nothing is running or waiting: the queue has drained.
    if (Uploader.runnings === 0) refreshListing();
    return;
  }
  Uploader.runnings++;
  let uploader = Uploader.queues.shift();
  if (!Uploader.auth) {
    Uploader.auth = true;
    try {
      await checkAuth();
    } catch {
      Uploader.auth = false;
    }
  }
  uploader.ajax();
}

/**
 * Add breadcrumb
 * @param {string} href
 * @param {string} uri_prefix
 */
function addBreadcrumb(href, uri_prefix) {
  const $breadcrumb = document.querySelector(".breadcrumb");
  let parts = [];
  if (href === "/") {
    parts = [""];
  } else {
    parts = href.split("/");
  }
  const len = parts.length;
  let path = uri_prefix;
  let items = "";
  for (let i = 0; i < len; i++) {
    const name = parts[i];
    if (i > 0) {
      if (!path.endsWith("/")) {
        path += "/";
      }
      path += encodeURIComponent(name);
    }
    const escapedName = escapeHtml(name);
    const escapedPath = escapeHtml(path);
    const current = i === len - 1 ? ` aria-current="page"` : "";
    let item;
    if (i === 0) {
      item = `<a href="${escapedPath}" title="Root" aria-label="Root"${current}><svg aria-hidden="true" width="16" height="16" viewBox="0 0 16 16"><path d="M6.5 14.5v-3.505c0-.245.25-.495.5-.495h2c.25 0 .5.25.5.5v3.5a.5.5 0 0 0 .5.5h4a.5.5 0 0 0 .5-.5v-7a.5.5 0 0 0-.146-.354L13 5.793V2.5a.5.5 0 0 0-.5-.5h-1a.5.5 0 0 0-.5.5v1.293L8.354 1.146a.5.5 0 0 0-.708 0l-6 6A.5.5 0 0 0 1.5 7.5v7a.5.5 0 0 0 .5.5h4a.5.5 0 0 0 .5-.5z"/></svg></a>`;
    } else if (i === len - 1) {
      item = `<b${current}>${escapedName}</b>`;
    } else {
      item = `<a href="${escapedPath}">${escapedName}</a>`;
    }
    items += `<li>${item}</li>`;
  }
  $breadcrumb.innerHTML = `<ol>${items}</ol>`;
}

async function setupIndexPage() {
  if (DATA.allow_archive) {
    const $download = document.querySelector(".download");
    $download.href = baseUrl() + "?zip";
    $download.title = "Download folder as a .zip file";
    $download.classList.add("dlwt");
    $download.classList.remove("hidden");
  }

  if (DATA.allow_upload) {
    setupDropzone();
    setupUploadFile();
    setupNewFolder();
    setupNewFile();
  }

  if (DATA.auth) {
    await setupAuth();
  }

  if (DATA.allow_search) {
    setupSearch();
  }

  $pathsTableBody.addEventListener("click", onPathsTableClick);
  if (DATA.user) {
    setupDownloadWithToken();
  }

  setupSelection();
  renderPathsTableHead();
  renderPathsTableBody();
}

/**
 * Render path table thead
 */
function renderPathsTableHead() {
  const headerItems = [
    {
      name: "name",
      props: ``,
      text: "Name",
    },
    {
      name: "mtime",
      props: ``,
      text: "Last Modified",
    },
    {
      name: "size",
      props: ``,
      text: "Size",
    }
  ];
  $pathsTableHead.insertAdjacentHTML("beforeend", `
    <tr>
      <th class="cell-select"><label class="select-box"><input type="checkbox" class="select-all" aria-label="Select all"><span class="select-all-text">Select all</span></label></th>
      <th class="cell-icon"></th>
      ${headerItems.map(item => {
    // The server sorts by name ascending when no sort is asked for, so the
    // default view is Name ascending, not an unsorted list.
    const sortName = PARAMS.sort || "name";
    const sortDesc = PARAMS.order === "desc";
    const active = sortName === item.name;
    const neutral = `<svg width="12" height="12" viewBox="0 0 16 16"><path fill-rule="evenodd" d="M11.5 15a.5.5 0 0 0 .5-.5V2.707l3.146 3.147a.5.5 0 0 0 .708-.708l-4-4a.5.5 0 0 0-.708 0l-4 4a.5.5 0 1 0 .708.708L11 2.707V14.5a.5.5 0 0 0 .5.5zm-7-14a.5.5 0 0 1 .5.5v11.793l3.146-3.147a.5.5 0 0 1 .708.708l-4 4a.5.5 0 0 1-.708 0l-4-4a.5.5 0 0 1 .708-.708L4 13.293V1.5a.5.5 0 0 1 .5-.5z"/></svg>`;
    const up = `<svg width="12" height="12" viewBox="0 0 16 16"><path fill-rule="evenodd" d="M8 1a.5.5 0 0 1 .5.5v11.793l3.146-3.147a.5.5 0 0 1 .708.708l-4 4a.5.5 0 0 1-.708 0l-4-4a.5.5 0 0 1 .708-.708L7.5 13.293V1.5A.5.5 0 0 1 8 1z"/></svg>`;
    const down = `<svg width="12" height="12" viewBox="0 0 16 16"><path fill-rule="evenodd" d="M8 15a.5.5 0 0 0 .5-.5V2.707l3.146 3.147a.5.5 0 0 0 .708-.708l-4-4a.5.5 0 0 0-.708 0l-4 4a.5.5 0 1 0 .708.708L7.5 2.707V14.5a.5.5 0 0 0 .5.5z"/></svg>`;
    const svg = active ? (sortDesc ? down : up) : neutral;
    // Upstream's behaviour, kept: the active column flips, any other column
    // starts descending.
    const order = active && sortDesc ? "asc" : "desc";
    const qs = new URLSearchParams({ ...PARAMS, order, sort: item.name }).toString();
    const icon = `<span>${svg}</span>`
    // The active column says so, for sighted and screen-reader users alike.
    const ariaSort = active ? `aria-sort="${sortDesc ? "descending" : "ascending"}"` : "";
    const cls = active ? " active" : "";
    return `<th class="cell-${item.name}${cls}" ${item.props} ${ariaSort}><a href="?${escapeHtml(qs)}">${item.text}${icon}</a></th>`
  }).join("\n")}
      <th class="cell-actions">Actions</th>
    </tr>
  `);
}

/**
 * The note shown instead of the paths table when there is nothing to list.
 */
function emptyNote() {
  if (DATA.paths.length > 0) return 'No matches in this folder — press Enter to search subfolders';
  return PARAMS.q ? 'No results' : DATA.dir_exists ? 'Empty folder' : 'Folder will be created when a file is uploaded';
}

/**
 * The entries of DATA.paths whose name contains the filter text, ignoring case.
 * @returns {PathItem[]}
 */
function visiblePaths() {
  const needle = filterText.trim().toLowerCase();
  if (!needle) return DATA.paths;
  return DATA.paths.filter(p => p.name.toLowerCase().includes(needle));
}

/**
 * Say how many entries the filter leaves, for screen readers.
 * @param {number} count
 */
function announceMatches(count) {
  $filterStatus.textContent = count === 0 ? 'No matches' : count === 1 ? '1 match' : `${count} matches`;
}

/**
 * Remember which control inside the paths table has the keyboard focus, as the
 * row's name and the control's position in the row.
 * @returns {{name: string | null, slot: number} | null}
 */
function captureFocus() {
  const active = document.activeElement;
  if (!active || !$pathsTableBody.contains(active)) return null;
  const row = active.closest("tr");
  return { name: row.dataset.name, slot: rowControls(row).indexOf(active) };
}

/**
 * @param {Element} row
 * @returns {Element[]}
 */
function rowControls(row) {
  return Array.from(row.querySelectorAll("a[href], button, input"));
}

/**
 * Put the focus back on the control `focus` describes, or on the row's first
 * control when the row has fewer. With no rows left, the empty-folder note
 * takes the focus so it does not fall back to the page.
 * @param {{name: string | null, slot: number} | null} focus
 */
function restoreFocus(focus) {
  if (!focus) return;
  if (visiblePaths().length === 0) {
    $emptyFolder.focus({ preventScroll: true });
    return;
  }
  const row = Array.from($pathsTableBody.rows).find(tr => tr.dataset.name === focus.name);
  if (!row) return;
  const controls = rowControls(row);
  (controls[focus.slot] || controls[0])?.focus({ preventScroll: true });
}

/**
 * Render path table tbody from DATA.paths, keeping the scroll position and the
 * keyboard focus.
 * @param {{name: string | null, slot: number} | null} [focus] where the focus
 * goes afterwards; defaults to where it is now
 */
function renderPathsTableBody(focus = captureFocus()) {
  const listed = new Set(DATA.paths.map(p => p.name));
  for (const name of selection) {
    if (!listed.has(name)) selection.delete(name);
  }
  const scrollY = window.scrollY;
  $pathsTableBody.replaceChildren();
  const paths = visiblePaths();
  if (paths.length > 0) {
    // Show an action column when it is available for the current directory or
    // at least one entry. Rows without permission render the action disabled.
    const showMove =
      (DATA.allow_upload && DATA.allow_delete) ||
      DATA.paths.some((p) => p.allow_upload && p.allow_delete);
    const showDelete =
      DATA.allow_delete || DATA.paths.some((p) => p.allow_delete);
    for (const file of paths) {
      addPath(file, showMove, showDelete);
    }
    $pathsTable.classList.remove("hidden");
    $emptyFolder.classList.add("hidden");
  } else {
    $pathsTable.classList.add("hidden");
    $emptyFolder.textContent = emptyNote();
    $emptyFolder.classList.remove("hidden");
  }
  if (filterText.trim()) announceMatches(paths.length);
  window.scrollTo(window.scrollX, scrollY);
  syncSelection();
  restoreFocus(focus);
}

/**
 * Whether a refresh is running, and whether another one was asked for while it
 * ran (the listing it fetched may predate that request).
 */
let listingRefresh = null;
let listingStale = false;

/**
 * Fetch the listing again, with the page's sort and search, and render it.
 * Calls made while a fetch is in flight collapse into one more fetch.
 * @returns {Promise<void>}
 */
function refreshListing() {
  if (listingRefresh) {
    listingStale = true;
    return listingRefresh;
  }
  listingRefresh = (async () => {
    try {
      do {
        listingStale = false;
        await fetchListing();
      } while (listingStale);
    } finally {
      listingRefresh = null;
    }
  })();
  return listingRefresh;
}

async function fetchListing() {
  const params = new URLSearchParams({ json: "" });
  for (const key of ["sort", "order", "q"]) {
    if (key in PARAMS) params.set(key, PARAMS[key]);
  }
  try {
    const res = await fetch(`${baseUrl()}?${params}`);
    await assertResOK(res);
    const data = await res.json();
    if (!Array.isArray(data.paths)) throw new Error("Unexpected response");
    DATA.paths = data.paths;
    DATA.dir_exists = data.dir_exists;
    clearNotice("listing");
    renderPathsTableBody();
  } catch (err) {
    // Leave the old listing in place.
    showNotice(`Could not refresh the listing: ${err.message}`, "listing");
  }
}

/**
 * Render an action button that the current account cannot use.
 * @param {string} icon
 * @param {string} title
 */
function disabledAction(icon, title) {
  const escapedTitle = escapeHtml(title);
  return `<span class="action-btn disabled" title="${escapedTitle}" aria-disabled="true" role="button" aria-label="${escapedTitle}">${icon}</span>`;
}

/**
 * Explain why an entry cannot be modified by the current account.
 * @param {PathItem} file
 * @param {"move" | "delete" | "edit"} action
 */
function deniedReason(file, action) {
  if (file.readonly_descendant) {
    return "the directory contains read-only entries";
  }
  if (action === "edit") {
    return "the file is read-only";
  }
  return "the entry is read-only";
}

/**
 * Add pathitem
 * @param {PathItem} file
 * @param {boolean} showMove
 * @param {boolean} showDelete
 */
function addPath(file, showMove, showDelete) {
  const escapedName = escapeHtml(file.name);
  let actionDelete = "";
  let actionDownload = "";
  let actionMove = "";
  let actionEdit = "";
  let actionView = "";
  const isDir = file.path_type.endsWith("Dir");
  const url = newUrl(file.name) + (isDir ? "/" : "");
  const escapedUrl = escapeHtml(url);
  if (isDir) {
    if (DATA.allow_archive) {
      actionDownload = `
      <div class="action-btn">
        <a class="dlwt" href="${escapedUrl}?zip" title="Download folder as a .zip file" download aria-label="Download ${escapedName} as a .zip file">${ICONS.download}</a>
      </div>`;
    }
  } else {
    actionDownload = `
    <div class="action-btn" >
      <a class="dlwt" href="${escapedUrl}" title="Download file" download aria-label="Download ${escapedName}">${ICONS.download}</a>
    </div>`;
  }
  const canMove = file.allow_upload && file.allow_delete;
  const canEdit = !isDir && canMove;
  if (showMove) {
    if (canMove) {
      actionMove = `<button type="button" data-action="move" class="action-btn" title="Move & Rename" aria-label="Move or rename ${escapedName}">${ICONS.move}</button>`;
    } else {
      actionMove = disabledAction(ICONS.move, `Cannot move: ${deniedReason(file, "move")}`);
    }
    if (!isDir) {
      if (canEdit) {
        actionEdit = `<a class="action-btn" title="Edit file" target="_blank" href="${escapedUrl}?edit" aria-label="Edit ${escapedName}">${ICONS.edit}</a>`;
      } else {
        actionEdit = disabledAction(ICONS.edit, `Cannot edit: ${deniedReason(file, "edit")}`);
      }
    }
  }
  if (showDelete) {
    if (file.allow_delete) {
      actionDelete = `
    <button type="button" data-action="delete" class="action-btn" title="Delete" aria-label="Delete ${escapedName}">${ICONS.delete}</button>`;
    } else {
      actionDelete = disabledAction(ICONS.delete, `Cannot delete: ${deniedReason(file, "delete")}`);
    }
  }
  if (!canEdit && !isDir) {
    actionView = `<a class="action-btn" title="View file" target="_blank" href="${escapedUrl}?view" aria-label="View ${escapedName}">${ICONS.view}</a>`;
  }
  // Every row gets the same five slots in the same order, so an action sits at
  // the same x on every row even when a row has fewer buttons (a folder has no
  // view or edit). Empty slots are invisible and only hold the column.
  const slot = (html) => (html ? html : `<span class="action-slot"></span>`);
  let actionCell = `
  <td class="cell-actions">
    ${slot(actionDownload)}
    ${slot(actionView)}
    ${slot(actionMove)}
    ${slot(actionDelete)}
    ${slot(actionEdit)}
  </td>`;

  let sizeDisplay = isDir ? formatDirSize(file.size) : formatFileSize(file.size).join(" ");

  $pathsTableBody.insertAdjacentHTML("beforeend", `
<tr data-name="${escapedName}">
  <td class="cell-select"><label class="select-box"><input type="checkbox" data-select aria-label="Select ${escapedName}"></label></td>
  <td class="path cell-icon">
    ${getPathSvg(file.path_type)}
  </td>
  <td class="path cell-name">
    <a href="${escapedUrl}" ${isDir ? "" : `target="_blank"`}>${escapedName}</a>
    <span class="path-meta">${formatMtime(file.mtime)} · ${sizeDisplay.trim()}</span>
  </td>
  <td class="cell-mtime">${formatMtime(file.mtime)}</td>
  <td class="cell-size">${sizeDisplay}</td>
  ${actionCell}
</tr>`);
}

/**
 * @param {DragEvent} e
 * @returns {boolean} whether the drag carries files, as opposed to text or links
 */
function dragHasFiles(e) {
  return !!e.dataTransfer && Array.from(e.dataTransfer.types).includes("Files");
}

function setupDropzone() {
  // Purely visual: hidden from assistive technology, the drop result is
  // announced through the live region. It ignores the pointer so it never
  // becomes the target of the drag events it is reacting to.
  const $overlay = document.createElement("div");
  $overlay.className = "drop-overlay";
  $overlay.setAttribute("aria-hidden", "true");
  const $message = document.createElement("div");
  $message.className = "drop-overlay-message";
  const $folder = document.createElement("strong");
  $folder.textContent = DATA.href;
  $message.append("Drop to upload to ", $folder);
  $overlay.append($message);
  document.body.append($overlay);

  // dragenter/dragleave fire for every element crossed, so count them and
  // show the overlay only while the drag is inside the window.
  let depth = 0;
  const hideOverlay = () => {
    depth = 0;
    $overlay.classList.remove("visible");
  };

  document.addEventListener("dragenter", e => {
    e.preventDefault();
    if (!dragHasFiles(e)) return;
    depth++;
    $overlay.classList.add("visible");
  });
  document.addEventListener("dragleave", e => {
    e.preventDefault();
    if (!dragHasFiles(e)) return;
    depth = Math.max(depth - 1, 0);
    if (depth === 0) hideOverlay();
  });
  document.addEventListener("dragover", e => {
    e.preventDefault();
    // only files can be dropped here: show "not allowed" for anything else
    if (e.dataTransfer) e.dataTransfer.dropEffect = dragHasFiles(e) ? "copy" : "none";
  });
  document.addEventListener("dragend", hideOverlay);
  document.addEventListener("keydown", e => {
    if (e.key === "Escape") hideOverlay();
  });
  document.addEventListener("drop", e => {
    e.preventDefault();
    hideOverlay();
    // DataTransferItem entries are only valid during the event dispatch, so
    // everything below must run synchronously.
    const items = Array.from(e.dataTransfer.items).filter(item => item.kind === "file");
    if (items.length === 0) return;
    announce(`Dropped ${items.length === 1 ? "1 item" : `${items.length} items`} to upload to ${DATA.href}`);
    if (typeof items[0].webkitGetAsEntry !== "function") {
      let skippedFolder = false;
      for (const item of items) {
        const file = item.getAsFile();
        if (!file) continue;
        // browsers without the entries API report folders as empty, untyped files
        if (file.size === 0 && file.type === "") {
          skippedFolder = true;
          continue;
        }
        new Uploader(file, []).upload();
      }
      if (skippedFolder) alert("Folders cannot be uploaded in this browser");
    } else {
      addFileEntries(items.map(item => item.webkitGetAsEntry()).filter(entry => entry), []);
    }
  });
}

async function setupAuth() {
  if (DATA.user) {
    $logoutBtn.classList.remove("hidden");
    $logoutBtn.addEventListener("click", logout);
    $userName.textContent = DATA.user;
  } else {
    $loginBtn.classList.remove("hidden");
    $loginBtn.addEventListener("click", async () => {
      try {
        await checkAuth("login");
      } catch { }
      location.reload();
    });
  }
}

/**
 * Download through a one-time token so the browser need not hold the
 * credentials. One listener on the document covers the toolbar link and the
 * rows, however often the rows are rendered.
 */
function setupDownloadWithToken() {
  document.addEventListener("click", async e => {
    const link = e.target.closest("a.dlwt");
    if (!link) return;
    e.preventDefault();
    try {
      await startDownload(link.getAttribute("href"));
    } catch (err) {
      showNotice(`Failed to download, ${err.message}`);
    }
  });
}

/**
 * Start the browser's download of a URL: through a fresh one-time token when
 * a user is logged in, directly otherwise.
 * @param {string} href
 * @param {string} [filename]  suggested name; the server's wins when it sends one
 */
async function startDownload(href, filename = "") {
  let url = href;
  if (DATA.user) {
    const tokengenUrl = new URL(href);
    tokengenUrl.searchParams.set("tokengen", "");
    const res = await fetch(tokengenUrl);
    if (!res.ok) throw new Error("Failed to fetch token");
    const token = await res.text();
    const downloadUrl = new URL(href);
    downloadUrl.searchParams.set("token", token);
    url = downloadUrl.toString();
  }
  const tempA = document.createElement("a");
  tempA.href = url;
  tempA.download = filename;
  document.body.appendChild(tempA);
  tempA.click();
  document.body.removeChild(tempA);
}

function setupSearch() {
  const $searchbar = document.querySelector(".searchbar");
  $searchbar.classList.remove("hidden");
  $searchbar.addEventListener("submit", event => {
    event.preventDefault();
    const q = new FormData($searchbar).get("q");
    const params = new URLSearchParams();
    if (q.trim()) {
      params.set("q", q);
    }
    if (PARAMS.sort) params.set("sort", PARAMS.sort);
    if (PARAMS.order) params.set("order", PARAMS.order);
    const qs = params.toString();
    location.href = qs ? `${baseUrl()}?${qs}` : baseUrl();
  });
  const $search = document.getElementById("search");
  if (PARAMS.q) {
    $search.value = PARAMS.q;
  }

  // Typing narrows the rows of this folder; Enter (above) searches subfolders.
  let debounce;
  const applyFilter = () => {
    clearTimeout(debounce);
    filterText = $search.value;
    renderPathsTableBody();
    if (!filterText.trim()) {
      $filterStatus.textContent = `Showing all ${DATA.paths.length} entries`;
    }
  };
  $search.addEventListener("input", () => {
    clearTimeout(debounce);
    debounce = setTimeout(applyFilter, 100);
  });
  $search.addEventListener("keydown", event => {
    if (event.key !== "Escape" || !$search.value) return;
    event.preventDefault();
    $search.value = "";
    applyFilter();
  });
  document.addEventListener("keydown", event => {
    if (event.key !== "/" || event.ctrlKey || event.metaKey || event.altKey || event.isComposing) return;
    const target = event.target;
    if (target instanceof Element && target.closest("input, textarea, select, [contenteditable]")) return;
    event.preventDefault();
    $search.focus();
    $search.select();
  });
}

function setupUploadFile() {
  document.querySelector(".upload-file").classList.remove("hidden");
  document.getElementById("file").addEventListener("change", async e => {
    const files = e.target.files;
    for (let file of files) {
      new Uploader(file, []).upload();
    }
  });
}

function setupNewFolder() {
  const $newFolder = document.querySelector(".new-folder");
  $newFolder.classList.remove("hidden");
  $newFolder.addEventListener("click", async () => {
    const name = await promptDialog("New folder", "Folder name", "", nameValidator(n => `A file or folder named \u201c${n}\u201d already exists here.`), {
      confirmLabel: "Create",
    });
    if (name) createFolder(name);
  });
}

function setupNewFile() {
  const $newFile = document.querySelector(".new-file");
  $newFile.classList.remove("hidden");
  $newFile.addEventListener("click", async () => {
    const name = await promptDialog("New file", "File name", "", nameValidator(n => `\u201c${n}\u201d already exists here and would be replaced.`), {
      confirmLabel: "Create",
    });
    if (name) createFile(name);
  });
}

async function setupEditorPage() {
  const url = baseUrl();

  const $download = document.querySelector(".download");
  $download.classList.remove("hidden");
  $download.href = url;

  if (DATA.kind == "Edit") {
    if (DATA.allow_upload && DATA.allow_delete) {
      const $moveFile = document.querySelector(".move-file");
      $moveFile.classList.remove("hidden");
      $moveFile.addEventListener("click", async () => {
        const query = location.href.slice(url.length);
        const newFileUrl = await doMovePath(url);
        if (newFileUrl) {
          location.href = newFileUrl + query;
        }
      });
    }

    if (DATA.allow_delete) {
      const $deleteFile = document.querySelector(".delete-file");
      $deleteFile.classList.remove("hidden");
      $deleteFile.addEventListener("click", async () => {
        const url = baseUrl();
        const name = baseName(url);
        await doDeletePath(name, url, () => {
          location.href = location.href.split("/").slice(0, -1).join("/");
        });
      });
    }

    if (DATA.editable && DATA.allow_upload) {
      $saveBtn = document.querySelector(".save-btn");
      $saveBtn.classList.remove("hidden");
      $saveBtn.addEventListener("click", () => saveChange(false));
      $overwriteBtn = document.querySelector(".editor-overwrite-btn");
      $overwriteBtn.addEventListener("click", () => saveChange(true));
      document.querySelector(".editor-reload-btn").addEventListener("click", () => {
        // the user chose to discard their edits, so do not warn about them
        editorSavedValue = $editor.value;
        location.reload();
      });
      document.addEventListener("keydown", saveShortcutHandler);
    }
  }

  if (!DATA.editable) {
    const $notEditable = document.querySelector(".not-editable");
    const url = baseUrl();
    const ext = extName(baseName(url));
    if (IFRAME_FORMATS.find(v => v === ext)) {
      $notEditable.insertAdjacentHTML("afterend", `<iframe src="${escapeHtml(url)}" sandbox width="100%" height="${window.innerHeight - 100}px"></iframe>`);
    } else {
      $notEditable.classList.remove("hidden");
      $notEditable.textContent = "Cannot edit because file is too large or binary.";
    }
    return;
  }

  $editor.classList.remove("hidden");
  const readOnly = DATA.kind == "View" || !DATA.allow_upload;
  $editor.readOnly = readOnly;
  if (readOnly) {
    $editor.setAttribute("aria-label", "File contents (read-only)");
  } else {
    $editor.setAttribute("aria-describedby", "editor-hint");
    $editor.addEventListener("keydown", editorTabHandler);
    $editor.addEventListener("focus", () => { editorTabTrap = true; });
    $editor.addEventListener("input", updateDirtyState);
  }
  try {
    const res = await fetch(baseUrl());
    await assertResOK(res);
    editorEtag = res.headers.get("etag");
    const encoding = getEncoding(res.headers.get("content-type"));
    if (encoding === "utf-8") {
      $editor.value = await res.text();
    } else {
      const bytes = await res.arrayBuffer();
      const dataView = new DataView(bytes);
      const decoder = new TextDecoder(encoding);
      $editor.value = decoder.decode(dataView);
    }
    editorSavedValue = $editor.value;
  } catch (err) {
    showNotice(`Failed to get file, ${err.message}`);
  }
}

/**
 * Run the action of a row button.
 * @param {MouseEvent} e
 */
function onPathsTableClick(e) {
  const box = e.target.closest("input[data-select]");
  if (box) {
    toggleSelected(box, e.shiftKey);
    return;
  }
  const button = e.target.closest("button[data-action]");
  if (!button) return;
  const name = button.closest("tr").dataset.name;
  if (button.dataset.action === "delete") {
    deletePath(name);
  } else if (button.dataset.action === "move") {
    movePath(name);
  }
}

/**
 * Delete path
 * @param {string} name
 * @returns
 */
async function deletePath(name) {
  const file = DATA.paths.find(p => p.name === name);
  if (!file) return;
  await doDeletePath(file.name, newUrl(file.name), () => {
    const shown = visiblePaths();
    const index = shown.indexOf(file);
    let focus = captureFocus();
    if (focus?.name === name) {
      // The focused row is going away: the next row (or the previous, for the
      // last one) takes the focus on the same control.
      const next = shown[index + 1] || shown[index - 1];
      focus = { name: next?.name ?? null, slot: focus.slot };
    }
    DATA.paths.splice(DATA.paths.indexOf(file), 1);
    renderPathsTableBody(focus);
    refreshListing();
  }, file);
}

/**
 * Confirm, then delete an entry.
 * @param {string} name
 * @param {string} url
 * @param {() => void} cb  runs after a successful delete
 * @param {PathItem} [entry]  the listed entry; absent for the open file
 */
async function doDeletePath(name, url, cb, entry) {
  const isDir = !!entry?.path_type.endsWith("Dir");
  const body = isDir
    ? `The folder \u201c${name}\u201d and everything in it (${formatDirSize(entry.size).trim()}) will be deleted. This cannot be undone.`
    : `The file \u201c${name}\u201d will be deleted. This cannot be undone.`;
  const confirmed = await confirmDialog(isDir ? "Delete folder?" : "Delete file?", body, {
    danger: true,
    confirmLabel: "Delete",
  });
  if (!confirmed) return;
  try {
    await checkAuth();
    const res = await fetch(url, {
      method: "DELETE",
    });
    await assertResOK(res);
    cb();
  } catch (err) {
    const blocked = deleteBlockedReason(err);
    if (blocked) {
      showNotice(`Cannot delete \`${name}\`: ${blocked}`);
    } else {
      showNotice(`Cannot delete \`${name}\`, ${err.message}`);
    }
  }
}

/**
 * Move path
 * @param {string} name
 * @returns
 */
async function movePath(name) {
  const file = DATA.paths.find(p => p.name === name);
  if (!file) return;
  const fileUrl = newUrl(file.name);
  const newFileUrl = await doMovePath(fileUrl, file.path_type.endsWith("Dir"));
  if (newFileUrl) {
    location.href = newFileUrl.split("/").slice(0, -1).join("/");
  }
}

/**
 * Ask for the new path of an entry and move it there.
 * @param {string} fileUrl
 * @param {boolean} [isDir]
 * @returns {Promise<string | undefined>} the new URL, or undefined when
 * cancelled or failed
 */
async function doMovePath(fileUrl, isDir = false) {
  const fileUrlObj = new URL(fileUrl);

  const prefix = DATA.uri_prefix.slice(0, -1);

  const filePath = decodeURIComponent(fileUrlObj.pathname.slice(prefix.length));
  const toUrl = path => fileUrlObj.origin + prefix + path.split("/").map(encodeURIComponent).join("/");

  // Select the base name only, without the extension of a file.
  const nameStart = filePath.lastIndexOf("/") + 1;
  const nameEnd = isDir ? filePath.length : filePath.length - extName(filePath.slice(nameStart)).length;

  const value = await promptDialog(
    isDir ? "Move or rename folder" : "Move or rename file",
    "New path",
    filePath,
    movePathValidator(filePath),
    {
      confirmLabel: "Move",
      selection: [nameStart, nameEnd],
      beforeSubmit: async value => {
        await checkAuth();
        const res = await fetch(toUrl(normalizeMovePath(value)), {
          method: "HEAD",
        });
        return res.status === 200 ? "Override existing file" : null;
      },
    },
  );
  if (value === null) return;
  const newPath = normalizeMovePath(value);
  const newFileUrl = toUrl(newPath);

  try {
    const res = await fetch(fileUrl, {
      method: "MOVE",
      headers: {
        "Destination": newFileUrl,
      }
    });
    await assertResOK(res);
    return newFileUrl;
  } catch (err) {
    const blocked = moveBlockedReason(err);
    if (blocked) {
      showNotice(`Cannot move \`${filePath}\`: ${blocked}`);
    } else {
      showNotice(`Cannot move \`${filePath}\` to \`${newPath}\`, ${err.message}`);
    }
  }
}

/**
 * Why the server refused a delete, when it said.
 * @param {Error & {code?: string | null}} err
 * @returns {string | null}
 */
function deleteBlockedReason(err) {
  if (err.code === "readonly-descendant") return "the directory contains read-only entries";
  if (err.code === "delete-disabled") return "delete is disabled";
  return null;
}

/**
 * Why the server refused a move, when it said.
 * @param {Error & {code?: string | null}} err
 * @returns {string | null}
 */
function moveBlockedReason(err) {
  if (err.code === "readonly-descendant") return "the directory contains read-only entries";
  if (err.code === "upload-disabled" || err.code === "delete-disabled") return "moving is disabled";
  return null;
}

/**
 * @param {PathItem} file
 */
function entryIsDir(file) {
  return file.path_type.endsWith("Dir");
}

/**
 * The last segment of a listed name; searches list entries by relative path.
 * @param {PathItem} file
 */
function entryLeaf(file) {
  return file.name.split("/").pop();
}

function setupSelection() {
  $pathsTableHead.addEventListener("change", e => {
    if (!e.target.matches(".select-all")) return;
    selection.clear();
    if (e.target.checked) {
      for (const file of DATA.paths) selection.add(file.name);
    }
    selectAnchor = null;
    syncSelection();
  });
  document.querySelector(".selection-bar").addEventListener("click", onBulkClick);
  document.addEventListener("keydown", e => {
    // A dialog takes the Escape for itself.
    if (e.key !== "Escape" || selection.size === 0 || document.querySelector("dialog[open]")) return;
    clearSelection();
  });
  // The selection belongs to this folder; it must not come back with the page
  // from the back/forward cache.
  window.addEventListener("pagehide", clearSelection);
}

/**
 * Select or deselect the row of a clicked checkbox, and the whole range from
 * the last clicked row with Shift.
 * @param {HTMLInputElement} box  already toggled by the click
 * @param {boolean} shift
 */
function toggleSelected(box, shift) {
  const names = Array.from($pathsTableBody.rows, tr => tr.dataset.name);
  const name = box.closest("tr").dataset.name;
  let range = [name];
  if (shift && selectAnchor !== null && names.includes(selectAnchor)) {
    const from = names.indexOf(selectAnchor);
    const to = names.indexOf(name);
    range = names.slice(Math.min(from, to), Math.max(from, to) + 1);
  }
  for (const n of range) {
    if (box.checked) selection.add(n);
    else selection.delete(n);
  }
  selectAnchor = name;
  syncSelection();
}

function clearSelection() {
  selection.clear();
  selectAnchor = null;
  syncSelection();
}

/**
 * Bring the checkboxes, the row highlight and the action bar in line with
 * `selection`.
 */
function syncSelection() {
  // The refusal it explained is about a selection that no longer exists.
  clearNotice("bulk-refused");
  const rows = Array.from($pathsTableBody.rows);
  for (const tr of rows) {
    const on = selection.has(tr.dataset.name);
    tr.classList.toggle("selected", on);
    tr.querySelector("input[data-select]").checked = on;
  }
  const $all = $pathsTableHead.querySelector(".select-all");
  $all.checked = rows.length > 0 && selection.size === rows.length;
  $all.indeterminate = selection.size > 0 && selection.size < rows.length;
  renderSelectionBar();
}

/**
 * @returns {PathItem[]}
 */
function selectedEntries() {
  return DATA.paths.filter(p => selection.has(p.name));
}

/**
 * Explain why `action` is refused when some selected entry does not allow it.
 * @param {PathItem[]} entries
 * @param {(file: PathItem) => boolean} allowed
 * @param {"move" | "delete"} action
 * @returns {string | null}
 */
function refusedReason(entries, allowed, action) {
  const refused = entries.filter(file => !allowed(file));
  if (refused.length === 0) return null;
  const more = refused.length > 1 ? ` and ${refused.length - 1} more` : "";
  return `Cannot ${action} \u201c${refused[0].name}\u201d${more}: ${deniedReason(refused[0], action)}`;
}

/**
 * Show the action bar when something is selected, with the actions every
 * selected entry allows. A refused action stays focusable, with the reason as
 * its tooltip.
 */
function renderSelectionBar() {
  const entries = selectedEntries();
  const $bar = document.querySelector(".selection-bar");
  $bar.classList.toggle("hidden", entries.length === 0);
  if (entries.length === 0) return;
  $bar.querySelector(".selection-count").textContent = `${entries.length} selected`;

  /**
   * @param {string} action
   * @param {string | null} reason  why the action is refused, if it is
   * @param {string} title  the tooltip of an allowed action
   */
  const setButton = (action, reason, title) => {
    const $button = $bar.querySelector(`[data-bulk="${action}"]`);
    $button.title = reason || title;
    if (reason) $button.setAttribute("aria-disabled", "true");
    else $button.removeAttribute("aria-disabled");
  };

  const dirs = entries.filter(entryIsDir).length;
  const files = entries.length - dirs;
  let downloadReason = null;
  let downloadTitle = `Download ${entries.length} selected ${entries.length === 1 ? "entry" : "entries"}, one by one`;
  if (dirs > 0 && !DATA.allow_archive) {
    if (files === 0) {
      downloadReason = "Cannot download folders: .zip download is disabled";
    } else {
      downloadTitle = `Download the ${files} selected ${files === 1 ? "file" : "files"} one by one; folders are skipped because .zip download is disabled`;
    }
  }
  setButton("download", downloadReason, downloadTitle);
  setButton("move", refusedReason(entries, p => p.allow_upload && p.allow_delete, "move"), "Move the selected entries to another folder");
  setButton("delete", refusedReason(entries, p => p.allow_delete, "delete"), "Delete the selected entries");
}

/**
 * Run the action of an action bar button.
 * @param {MouseEvent} e
 */
async function onBulkClick(e) {
  const $button = e.target.closest("button[data-bulk]");
  if (!$button) return;
  const action = $button.dataset.bulk;
  if (action === "clear") {
    clearSelection();
    // The bar is gone with the selection; do not drop the focus on the page.
    $pathsTableHead.querySelector(".select-all").focus();
    return;
  }
  if (bulkBusy) return;
  if ($button.getAttribute("aria-disabled") === "true") {
    // Touch screens have no tooltip.
    showNotice($button.title, "bulk-refused");
    return;
  }
  const entries = selectedEntries();
  if (entries.length === 0) return;
  bulkBusy = true;
  const $bar = $button.closest(".selection-bar");
  $bar.setAttribute("aria-busy", "true");
  try {
    if (action === "download") await bulkDownload(entries);
    else if (action === "move") await bulkMove(entries);
    else if (action === "delete") await bulkDelete(entries);
  } finally {
    bulkBusy = false;
    $bar.removeAttribute("aria-busy");
  }
}

/**
 * Leave the focus somewhere sensible once the entries it was on are gone.
 */
function focusListing() {
  if (DATA.paths.length === 0) {
    $emptyFolder.focus({ preventScroll: true });
  } else {
    $pathsTableHead.querySelector(".select-all").focus({ preventScroll: true });
  }
}

/**
 * Drop entries that are gone from the page, and fetch the listing again.
 * @param {Set<string>} names
 */
function forgetEntries(names) {
  DATA.paths = DATA.paths.filter(p => !names.has(p.name));
  for (const name of names) selection.delete(name);
  selectAnchor = null;
  renderPathsTableBody(null);
  focusListing();
  refreshListing();
}

/**
 * Delete the selected entries, one request after the other. Failures are
 * collected and reported once.
 * @param {PathItem[]} entries
 */
async function bulkDelete(entries) {
  const hasDir = entries.some(entryIsDir);
  const count = `${entries.length} ${entries.length === 1 ? "entry" : "entries"}`;
  const names = entries.slice(0, CONFIRM_LIST_MAX).map(file => entryIsDir(file) ? `${file.name}/` : file.name);
  if (entries.length > CONFIRM_LIST_MAX) names.push(`and ${entries.length - CONFIRM_LIST_MAX} more`);
  const confirmed = await confirmDialog(
    `Delete ${count}?`,
    `${hasDir ? "The selected entries, and everything in the folders among them, will" : "The selected entries will"} be deleted. This cannot be undone.`,
    { danger: true, confirmLabel: "Delete", items: names },
  );
  if (!confirmed) return;
  try {
    await checkAuth();
  } catch (err) {
    showNotice(`Cannot delete, ${err.message}`);
    return;
  }
  const deleted = new Set();
  const failures = [];
  for (const file of entries) {
    try {
      const res = await fetch(newUrl(file.name), { method: "DELETE" });
      await assertResOK(res);
      deleted.add(file.name);
    } catch (err) {
      failures.push(`${file.name} (${deleteBlockedReason(err) || err.message})`);
    }
  }
  forgetEntries(deleted);
  if (failures.length > 0) {
    showNotice(`Could not delete ${failures.length} of ${entries.length}: ${failures.join("; ")}`);
  }
}

/**
 * Move the selected entries into a folder asked for once. An entry whose
 * target exists is left where it is and reported, like any other failure.
 * @param {PathItem[]} entries
 */
async function bulkMove(entries) {
  const prefix = DATA.uri_prefix.slice(0, -1);
  const here = decodeURIComponent(new URL(baseUrl()).pathname.slice(prefix.length)).replace(/\/+$/, "");
  const toUrl = path => location.origin + prefix + path.split("/").map(encodeURIComponent).join("/");
  const count = `${entries.length} ${entries.length === 1 ? "entry" : "entries"}`;

  const initial = `${here}/`;
  const value = await promptDialog(
    `Move ${count}`,
    "Destination folder",
    initial,
    value => {
      if (!value) return { level: "error", message: "Enter a folder." };
      const dest = normalizeMovePath(value);
      if (dest.split("/").slice(1).some(s => s === "" || s === "." || s === "..")) {
        return { level: "error", message: "A path cannot contain empty, \u201c.\u201d or \u201c..\u201d segments." };
      }
      if (entries.every(file => parentPath(`${here}/${file.name}`) === dest)) {
        return { level: "error", message: "The selected entries are already in that folder." };
      }
      return null;
    },
    { confirmLabel: "Move", selection: [initial.length, initial.length] },
  );
  if (value === null) return;
  const dest = normalizeMovePath(value);
  try {
    await checkAuth();
  } catch (err) {
    showNotice(`Cannot move, ${err.message}`);
    return;
  }

  const moved = new Set();
  const failures = [];
  for (const file of entries) {
    const from = `${here}/${file.name}`;
    const to = `${dest}/${entryLeaf(file)}`;
    try {
      if (to === from) throw new Error("it is already there");
      if (entryIsDir(file) && to.startsWith(`${from}/`)) throw new Error("a folder cannot move into itself");
      const head = await fetch(toUrl(to), { method: "HEAD" });
      if (head.status === 200) throw new Error("the target already exists");
      const res = await fetch(newUrl(file.name), {
        method: "MOVE",
        headers: { "Destination": toUrl(to) },
      });
      await assertResOK(res);
      moved.add(file.name);
    } catch (err) {
      failures.push(`${file.name} (${moveBlockedReason(err) || err.message})`);
    }
  }
  forgetEntries(moved);
  if (failures.length > 0) {
    showNotice(`Could not move ${failures.length} of ${entries.length} to ${dest || "/"}: ${failures.join("; ")}`);
  } else {
    showNotice(`Moved ${count} to ${dest || "/"}.`, "bulk-moved", "info");
  }
}

/**
 * Download the selected entries one by one: files as they are, folders as
 * .zip archives when the server offers them.
 * @param {PathItem[]} entries
 */
async function bulkDownload(entries) {
  const skipped = DATA.allow_archive ? [] : entries.filter(entryIsDir);
  const items = entries.filter(file => !skipped.includes(file));
  if (items.length > DOWNLOAD_CONFIRM_ABOVE) {
    const confirmed = await confirmDialog(
      `Download ${items.length} items?`,
      `Download ${items.length} items one by one? Your browser may ask to allow multiple downloads.`,
      { confirmLabel: "Download" },
    );
    if (!confirmed) return;
  }
  let started = 0;
  const failures = [];
  for (const file of items) {
    if (started + failures.length > 0) await new Promise(resolve => setTimeout(resolve, DOWNLOAD_GAP_MS));
    const isDir = entryIsDir(file);
    try {
      await startDownload(newUrl(file.name) + (isDir ? "/?zip" : ""), isDir ? `${entryLeaf(file)}.zip` : entryLeaf(file));
      started++;
    } catch (err) {
      failures.push(`${file.name} (${err.message})`);
    }
  }
  if (failures.length > 0) {
    showNotice(`Could not download ${failures.length} of ${items.length}: ${failures.join("; ")}`);
  }
  if (skipped.length > 0) {
    showNotice(`Skipped ${skipped.length} ${skipped.length === 1 ? "folder" : "folders"}: .zip download is disabled.`, "bulk-skipped", "info");
  }
  if (started > 1) {
    // A page cannot tell that the browser dropped a download.
    showNotice(
      `Started ${started} downloads. If only the first arrived, your browser blocked the rest: allow multiple (automatic) downloads for this site, in the site permissions next to the address bar or in the browser settings, then try again.`,
      "bulk-hint",
      "info",
    );
  }
}


/**
 * Save editor change
 */
async function saveChange(overwrite) {
  const hadFocus = document.activeElement === $saveBtn;
  $saveBtn.disabled = true;
  $overwriteBtn.disabled = true;
  clearTimeout(saveStatusTimer);
  hideEditorError();
  $saveStatus.textContent = "Saving…";
  const name = baseName(baseUrl());
  try {
    await checkAuth();
    const headers = {};
    if (editorEtag && !overwrite) {
      headers["If-Match"] = editorEtag;
    }
    const body = $editor.value;
    const res = await fetch(baseUrl(), {
      method: "PUT",
      headers,
      body,
    });
    await assertResOK(res);
    editorSavedValue = body;
    updateDirtyState();
    editorEtag = await fetchEtag();
    $saveStatus.textContent = "Saved";
    saveStatusTimer = setTimeout(() => {
      $saveStatus.textContent = "";
    }, 3000);
  } catch (err) {
    $saveStatus.textContent = "";
    if (err.code === "precondition-failed") {
      showEditorError(`Cannot save \`${name}\`: it was changed on the server after you opened it`, true);
    } else if (err.code === "upload-disabled") {
      showEditorError(`Cannot save \`${name}\`: upload is disabled`);
    } else if (err.code === "delete-disabled") {
      showEditorError(`Cannot save \`${name}\`: delete is disabled`);
    } else if (err.code === "readonly-descendant") {
      showEditorError(`Cannot save \`${name}\`: it is read-only`);
    } else if (err.status) {
      showEditorError(`Cannot save \`${name}\`, ${err.status} ${err.body}`.trimEnd());
    } else {
      showEditorError(`Cannot save \`${name}\`, ${err.message}`);
    }
  } finally {
    $saveBtn.disabled = false;
    $overwriteBtn.disabled = false;
    if (hadFocus || overwrite) $saveBtn.focus();
  }
}

/**
 * Whether the editor holds changes that were neither loaded nor saved.
 */
function isEditorDirty() {
  return !$editor.readOnly && $editor.value !== editorSavedValue;
}

/**
 * Reflect the dirty state on the Save button.
 */
function updateDirtyState() {
  if (!$saveBtn) return;
  const dirty = isEditorDirty();
  $saveBtn.querySelector(".save-dirty").classList.toggle("hidden", !dirty);
  const label = dirty ? "Save file, unsaved changes" : "Save file";
  $saveBtn.setAttribute("aria-label", label);
  $saveBtn.title = label;
}

/**
 * Ctrl+S / Cmd+S saves instead of opening the browser's save dialog.
 * @param {KeyboardEvent} event
 */
function saveShortcutHandler(event) {
  if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey || event.key.toLowerCase() !== "s") return;
  event.preventDefault();
  if (!$saveBtn.disabled) saveChange(false);
}

/**
 * Tab inserts a tab character while trapping is on; Escape or Ctrl+M turns
 * trapping off so keyboard users can leave the field.
 * @param {KeyboardEvent} event
 */
function editorTabHandler(event) {
  if (event.key === "Escape" || (event.ctrlKey && !event.metaKey && !event.altKey && event.key.toLowerCase() === "m")) {
    event.preventDefault();
    editorTabTrap = !editorTabTrap;
    return;
  }
  if (event.key !== "Tab" || !editorTabTrap || event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return;
  event.preventDefault();
  if (!document.execCommand("insertText", false, "\t")) {
    $editor.setRangeText("\t", $editor.selectionStart, $editor.selectionEnd, "end");
    $editor.dispatchEvent(new Event("input", { bubbles: true }));
  }
}

/**
 * Read the current ETag of the edited file, or null when it is unavailable.
 * @returns {Promise<string|null>}
 */
async function fetchEtag() {
  try {
    const res = await fetch(baseUrl(), { method: "HEAD", cache: "no-store" });
    return res.ok ? res.headers.get("etag") : null;
  } catch {
    return null;
  }
}

/**
 * Show a save failure above the editor, keeping the editor content intact.
 * @param {string} message
 * @param {boolean} [conflict] offer to reload or overwrite
 */
function showEditorError(message, conflict = false) {
  document.querySelector(".editor-error-text").textContent = message;
  document.querySelector(".editor-error-actions").classList.toggle("hidden", !conflict);
  document.querySelector(".editor-error").classList.remove("hidden");
}

function hideEditorError() {
  document.querySelector(".editor-error").classList.add("hidden");
}

async function checkAuth(variant) {
  if (!DATA.auth) return;
  const qs = variant ? `?${variant}` : "";
  const res = await fetch(baseUrl() + qs, {
    method: "CHECKAUTH",
  });
  await assertResOK(res);
  $loginBtn.classList.add("hidden");
  $logoutBtn.classList.remove("hidden");
  $userName.textContent = await res.text();
}

function logout() {
  if (!DATA.auth) return;
  const url = baseUrl();
  const xhr = new XMLHttpRequest();
  xhr.open("LOGOUT", url, true, DATA.user);
  xhr.onload = () => {
    location.href = url;
  }
  xhr.send();
}

/**
 * Create a folder
 * @param {string} name
 */
async function createFolder(name) {
  const url = newUrl(name);
  try {
    await checkAuth();
    const res = await fetch(url, {
      method: "MKCOL",
    });
    await assertResOK(res);
    location.href = url;
  } catch (err) {
    if (err.code === "already-exists") {
      showNotice(`Cannot create folder \`${name}\`: it already exists`);
    } else if (err.code === "upload-disabled") {
      showNotice(`Cannot create folder \`${name}\`: upload is disabled`);
    } else {
      showNotice(`Cannot create folder \`${name}\`, ${err.message}`);
    }
  }
}

async function createFile(name) {
  const url = newUrl(name);
  try {
    await checkAuth();
    const res = await fetch(url, {
      method: "PUT",
      body: "",
    });
    await assertResOK(res);
    location.href = url + "?edit";
  } catch (err) {
    if (err.code === "upload-disabled") {
      showNotice(`Cannot create file \`${name}\`: upload is disabled`);
    } else if (err.code === "delete-disabled") {
      showNotice(`Cannot create file \`${name}\`: delete is disabled`);
    } else {
      showNotice(`Cannot create file \`${name}\`, ${err.message}`);
    }
  }
}

function addFileEntries(entries, dirs) {
  for (const entry of entries) {
    if (entry.isFile) {
      entry.file(file => {
        new Uploader(file, dirs).upload();
      }, err => {
        Uploader.unreadable(entry.name, dirs, err.message || err.name);
      });
    } else if (entry.isDirectory) {
      const dirReader = entry.createReader();

      const successCallback = entries => {
        if (entries.length > 0) {
          addFileEntries(entries, [...dirs, entry.name]);
          dirReader.readEntries(successCallback, errorCallback);
        }
      };
      const errorCallback = err => {
        Uploader.unreadable(entry.name, dirs, err.message || err.name);
      };

      dirReader.readEntries(successCallback, errorCallback);
    }
  }
}


function newUrl(name) {
  let url = baseUrl();
  if (!url.endsWith("/")) url += "/";
  url += name.split("/").map(encodeURIComponent).join("/");
  return url;
}

function baseUrl() {
  return location.href.split(/[?#]/)[0];
}

function baseName(url) {
  return decodeURIComponent(url.split("/").filter(v => v.length > 0).slice(-1)[0]);
}

function extName(filename) {
  const dotIndex = filename.lastIndexOf('.');

  if (dotIndex === -1 || dotIndex === 0 || dotIndex === filename.length - 1) {
    return '';
  }

  return filename.substring(dotIndex);
}

function getPathSvg(path_type) {
  switch (path_type) {
    case "Dir":
      return ICONS.dir;
    case "SymlinkFile":
      return ICONS.symlinkFile;
    case "SymlinkDir":
      return ICONS.symlinkDir;
    default:
      return ICONS.file;
  }
}

function formatMtime(mtime) {
  if (!mtime) return "";
  const date = new Date(mtime);
  const year = date.getFullYear();
  const month = padZero(date.getMonth() + 1, 2);
  const day = padZero(date.getDate(), 2);
  const hours = padZero(date.getHours(), 2);
  const minutes = padZero(date.getMinutes(), 2);
  return `${year}-${month}-${day} ${hours}:${minutes}`;
}

function padZero(value, size) {
  return ("0".repeat(size) + value).slice(-1 * size);
}

function formatDirSize(size) {
  const unit = size === 1 ? "item" : "items";
  const num = size >= MAX_SUBPATHS_COUNT ? `>${MAX_SUBPATHS_COUNT - 1}` : `${size}`;
  return ` ${num} ${unit}`;
}

function formatFileSize(size, unit) {
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.max(0, unit ? sizes.indexOf(unit) : (size > 0 ? parseInt(Math.floor(Math.log(size) / Math.log(1024))) : 0));
  const raw = size > 0 ? size / Math.pow(1024, i) : 0;
  if (i === 0) return [Math.round(raw), "B"];
  return [raw.toFixed(1), sizes[i]];
}

function formatDuration(seconds) {
  if (!isFinite(seconds) || seconds <= 0) return "--:--";
  seconds = Math.ceil(seconds);
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds - h * 3600) / 60);
  const s = seconds - h * 3600 - m * 60;
  if (h > 0) return `${padZero(h, 2)}:${padZero(m, 2)}:${padZero(s, 2)}`;
  return `${padZero(m, 2)}:${padZero(s, 2)}`;
}

function formatUploadSize(uploaded, total) {
  const unit = formatFileSize(total)[1];
  const [uploadedValue] = formatFileSize(uploaded, unit);
  const [totalValue] = formatFileSize(total, unit);
  return `${uploadedValue}/${totalValue} ${unit}`;
}

function formatPercent(percent) {
  return Math.round(percent) + "%";
}

const HTML_ESCAPES = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };

/**
 * Escape a string for use in HTML text and in quoted attribute values.
 * @param {string} rawStr
 */
function escapeHtml(rawStr) {
  return String(rawStr).replace(/[&<>"']/g, c => HTML_ESCAPES[c]);
}

/**
 * Create an element, optionally with a class name and text content.
 * @param {string} tag
 * @param {string} [className]
 * @param {string} [text]
 */
function createElement(tag, className, text) {
  const $el = document.createElement(tag);
  if (className) $el.className = className;
  if (text !== undefined) $el.textContent = text;
  return $el;
}

let dialogSeq = 0;

/**
 * Build the shell shared by every dialog: a title, a body slot and a Cancel /
 * confirm button pair. The confirm button submits the form; Cancel closes the
 * dialog without a result, like Escape does.
 * @param {string} title
 * @param {string} confirmLabel
 * @param {boolean} danger
 */
function createDialog(title, confirmLabel, danger) {
  const id = ++dialogSeq;
  const $dialog = createElement("dialog", "dialog");
  $dialog.setAttribute("aria-labelledby", `dialog-title-${id}`);
  const $form = createElement("form", "dialog-form");
  $form.noValidate = true;
  const $title = createElement("h2", "dialog-title", title);
  $title.id = `dialog-title-${id}`;
  const $body = createElement("div", "dialog-body");
  const $actions = createElement("div", "dialog-actions");
  const $cancel = createElement("button", "dialog-btn", "Cancel");
  $cancel.type = "button";
  $cancel.addEventListener("click", () => $dialog.close());
  const $ok = createElement("button", danger ? "dialog-btn dialog-ok danger" : "dialog-btn dialog-ok", confirmLabel);
  $ok.type = "submit";
  $actions.append($cancel, $ok);
  $form.append($title, $body, $actions);
  $dialog.append($form);
  return { id, $dialog, $form, $body, $cancel, $ok };
}

/**
 * Open a dialog modally. Resolves with true once it is closed through its
 * confirm button (`close("ok")`) and false when it is cancelled.
 * @param {HTMLDialogElement} $dialog
 * @returns {Promise<boolean>}
 */
function showDialog($dialog) {
  return new Promise(resolve => {
    $dialog.addEventListener("close", () => {
      $dialog.remove();
      resolve($dialog.returnValue === "ok");
    }, { once: true });
    document.body.append($dialog);
    $dialog.showModal();
  });
}

/**
 * Ask for confirmation. Resolves with true when confirmed. A danger dialog
 * starts with the focus on Cancel.
 * @param {string} title
 * @param {string} body
 * @param {{danger?: boolean, confirmLabel?: string, items?: string[]}} [options]
 * `items` are listed under the text
 * @returns {Promise<boolean>}
 */
function confirmDialog(title, body, { danger = false, confirmLabel = "OK", items } = {}) {
  const d = createDialog(title, confirmLabel, danger);
  const $text = createElement("p", "dialog-text", body);
  $text.id = `dialog-text-${d.id}`;
  let describedBy = $text.id;
  d.$body.append($text);
  if (items) {
    const $list = createElement("ul", "dialog-list");
    $list.id = `dialog-list-${d.id}`;
    for (const item of items) $list.append(createElement("li", undefined, item));
    d.$body.append($list);
    describedBy += ` ${$list.id}`;
  }
  d.$dialog.setAttribute("aria-describedby", describedBy);
  d.$form.addEventListener("submit", e => {
    e.preventDefault();
    d.$dialog.close("ok");
  });
  const closed = showDialog(d.$dialog);
  (danger ? d.$cancel : d.$ok).focus();
  return closed;
}

/**
 * @typedef {object} Problem
 * @property {"error"|"warn"} level  errors block the submit, warnings do not
 * @property {string} message
 */

/**
 * Ask for a line of text. Resolves with the trimmed text, or null when
 * cancelled. `validate` runs on every input and again on submit.
 * `beforeSubmit` may resolve with the label of an acknowledgement checkbox
 * that has to be ticked, in the same dialog, before the dialog accepts.
 * @param {string} title
 * @param {string} label
 * @param {string} initial
 * @param {(value: string) => Problem | null} validate
 * @param {{confirmLabel?: string, selection?: [number, number], beforeSubmit?: (value: string) => Promise<string | null>}} [options]
 * @returns {Promise<string | null>}
 */
function promptDialog(title, label, initial, validate, { confirmLabel = "OK", selection, beforeSubmit } = {}) {
  const d = createDialog(title, confirmLabel, false);
  const $label = createElement("label", "dialog-label", label);
  $label.htmlFor = `dialog-input-${d.id}`;
  const $input = createElement("input", "dialog-input");
  $input.id = `dialog-input-${d.id}`;
  $input.type = "text";
  $input.value = initial;
  $input.autocomplete = "off";
  $input.spellcheck = false;
  $input.setAttribute("autocapitalize", "off");
  const $msg = createElement("p", "dialog-msg");
  $msg.id = `dialog-msg-${d.id}`;
  $msg.setAttribute("aria-live", "polite");
  $input.setAttribute("aria-describedby", $msg.id);
  const $ack = createElement("label", "dialog-ack hidden");
  const $check = createElement("input");
  $check.type = "checkbox";
  const $ackText = createElement("span");
  $ack.append($check, $ackText);
  d.$body.append($label, $input, $msg, $ack);

  let result = null;
  // The value the acknowledgement checkbox was asked for.
  let ackFor = null;
  let busy = false;

  /** @param {Problem | null} problem */
  const showProblem = problem => {
    $msg.textContent = problem ? problem.message : "";
    $msg.dataset.level = problem ? problem.level : "";
    if (problem?.level === "error") {
      $input.setAttribute("aria-invalid", "true");
    } else {
      $input.removeAttribute("aria-invalid");
    }
  };

  $input.addEventListener("input", () => {
    const value = $input.value.trim();
    showProblem(validate(value));
    if (value !== ackFor) {
      ackFor = null;
      $ack.classList.add("hidden");
    }
  });

  d.$form.addEventListener("submit", async e => {
    e.preventDefault();
    if (busy) return;
    const value = $input.value.trim();
    const problem = validate(value);
    showProblem(problem);
    if (problem?.level === "error") {
      $input.focus();
      return;
    }
    if (ackFor === value) {
      if (!$check.checked) {
        showProblem({ level: "error", message: "Tick the box to continue." });
        $check.focus();
        return;
      }
    } else if (beforeSubmit) {
      busy = true;
      d.$ok.setAttribute("aria-disabled", "true");
      let ackLabel;
      try {
        ackLabel = await beforeSubmit(value);
      } catch (err) {
        showProblem({ level: "error", message: err.message });
        $input.focus();
        return;
      } finally {
        busy = false;
        d.$ok.removeAttribute("aria-disabled");
      }
      // Edited while checking: the answer is for another value.
      if ($input.value.trim() !== value) return;
      if (ackLabel) {
        ackFor = value;
        $check.checked = false;
        $ackText.textContent = ackLabel;
        $ack.classList.remove("hidden");
        $check.focus();
        return;
      }
    }
    result = value;
    d.$dialog.close("ok");
  });

  const closed = showDialog(d.$dialog);
  $input.focus();
  if (selection) {
    $input.setSelectionRange(selection[0], selection[1]);
  } else {
    $input.select();
  }
  return closed.then(ok => (ok ? result : null));
}

/**
 * Show a dismissible notice at the top of the main area. A notice with the
 * same key is replaced; the key defaults to the message.
 * @param {string} message
 * @param {string} [key]
 * @param {"error" | "info"} [level]
 */
function showNotice(message, key = message, level = "error") {
  clearNotice(key);
  const $notice = createElement("div", "notice");
  $notice.dataset.key = key;
  $notice.dataset.level = level;
  const $dismiss = createElement("button", "notice-dismiss", "\u00d7");
  $dismiss.type = "button";
  $dismiss.setAttribute("aria-label", "Dismiss");
  $dismiss.addEventListener("click", () => $notice.remove());
  $notice.append(createElement("span", "notice-text", message), $dismiss);
  document.querySelector(".notices").append($notice);
  $notice.scrollIntoView({ block: "nearest" });
}

/**
 * Remove the notice shown under `key`, if any.
 * @param {string} key
 */
function clearNotice(key) {
  for (const $notice of document.querySelectorAll(".notice")) {
    if ($notice.dataset.key === key) $notice.remove();
  }
}

/**
 * Validator for the name of a new entry: no slash, not `.` or `..`. A name
 * that is already listed is a warning, not an error.
 * @param {(name: string) => string} existsMessage
 * @returns {(name: string) => Problem | null}
 */
function nameValidator(existsMessage) {
  return name => {
    if (!name) return { level: "error", message: "Enter a name." };
    if (name.includes("/")) return { level: "error", message: "A name cannot contain \u201c/\u201d." };
    if (name === "." || name === "..") return { level: "error", message: `\u201c${name}\u201d is not a valid name.` };
    if (DATA.paths.some(p => p.name === name)) return { level: "warn", message: existsMessage(name) };
    return null;
  };
}

/**
 * Normalise the text typed into the move dialog to an absolute path without a
 * trailing slash ("" is the root).
 * @param {string} value
 */
function normalizeMovePath(value) {
  const path = value.startsWith("/") ? value : "/" + value;
  return path.replace(/\/+$/, "");
}

/**
 * @param {string} path  absolute path without a trailing slash
 */
function parentPath(path) {
  return path.slice(0, path.lastIndexOf("/"));
}

/**
 * Validator for the move dialog. Slashes are allowed, `.` and `..` segments
 * are not. A target next to the moved entry that is already listed is a
 * warning.
 * @param {string} filePath  current path of the moved entry
 * @returns {(value: string) => Problem | null}
 */
function movePathValidator(filePath) {
  return value => {
    const newPath = normalizeMovePath(value);
    if (!newPath) return { level: "error", message: "Enter a path." };
    const segments = newPath.split("/").slice(1);
    if (segments.some(s => s === "" || s === "." || s === "..")) {
      return { level: "error", message: "A path cannot contain empty, \u201c.\u201d or \u201c..\u201d segments." };
    }
    if (newPath === filePath) return { level: "error", message: "That is the current path." };
    const name = segments[segments.length - 1];
    if (parentPath(newPath) === parentPath(filePath) && DATA.paths.some(p => p.name === name)) {
      return { level: "warn", message: `\u201c${name}\u201d already exists here.` };
    }
    return null;
  };
}

async function assertResOK(res) {
  if (!(res.status >= 200 && res.status < 300)) {
    const body = await res.text();
    const err = new Error(body || `Invalid status ${res.status}`);
    err.status = res.status;
    err.body = body;
    err.code = res.headers.get("x-dufs-error-code");
    throw err;
  }
}

function getEncoding(contentType) {
  const charset = contentType?.split(";")[1];
  if (/charset/i.test(charset)) {
    let encoding = charset.split("=")[1];
    if (encoding) {
      return encoding.toLowerCase();
    }
  }
  return 'utf-8';
}

// Parsing base64 strings with Unicode characters
function decodeBase64(base64String) {
  const binString = atob(base64String);
  const len = binString.length;
  const bytes = new Uint8Array(len);
  const arr = new Uint32Array(bytes.buffer, 0, Math.floor(len / 4));
  let i = 0;
  for (; i < arr.length; i++) {
    arr[i] = binString.charCodeAt(i * 4) |
      (binString.charCodeAt(i * 4 + 1) << 8) |
      (binString.charCodeAt(i * 4 + 2) << 16) |
      (binString.charCodeAt(i * 4 + 3) << 24);
  }
  for (i = i * 4; i < len; i++) {
    bytes[i] = binString.charCodeAt(i);
  }
  return new TextDecoder().decode(bytes);
}
