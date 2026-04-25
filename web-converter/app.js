import { h, render } from "https://esm.sh/preact@10.27.2";
import { useEffect, useMemo, useRef, useState } from "https://esm.sh/preact@10.27.2/hooks";
import initConverter, {
  convert_scorify_file_wasm,
  convert_scorify_text_wasm,
  convert_musicxml_file_wasm,
  convert_musicxml_text_with_options_wasm,
} from "./vendor/converter/musicxml_to_scorify.js";

const SAMPLE_TYPST = `#set page(margin: 16mm)
#import "/scorify/lib.typ": score

#score(
  title: "Simple Scorify Sample",
  composer: "MusicXML Fixture",
  key: "C",
  time: "4/4",
  staves: (
    (
      clef: "treble",
      instrument-name: "Flute",
      instrument-name-cont: "Fl.",
      music: "c4v[mf]l[Hel-] d4l[lo] e4*[G7] r4 |: f2~ f2 :|",
    ),
  ),
)`;

const SAMPLE_XML = `<?xml version="1.0" encoding="UTF-8"?>
<score-partwise version="4.0">
  <work><work-title>Simple Scorify Sample</work-title></work>
  <identification><creator type="composer">MusicXML Fixture</creator></identification>
  <part-list>
    <score-part id="P1">
      <part-name>Flute</part-name>
      <part-abbreviation>Fl.</part-abbreviation>
    </score-part>
  </part-list>
  <part id="P1">
    <measure number="1">
      <attributes>
        <divisions>2</divisions>
        <key><fifths>0</fifths></key>
        <time><beats>4</beats><beat-type>4</beat-type></time>
        <clef><sign>G</sign><line>2</line></clef>
      </attributes>
      <direction><direction-type><dynamics><mf/></dynamics></direction-type></direction>
      <note>
        <pitch><step>C</step><octave>4</octave></pitch>
        <duration>2</duration><type>quarter</type>
        <lyric><syllabic>begin</syllabic><text>Hel</text></lyric>
      </note>
      <note>
        <pitch><step>D</step><octave>4</octave></pitch>
        <duration>2</duration><type>quarter</type>
        <lyric><syllabic>end</syllabic><text>lo</text></lyric>
      </note>
      <harmony><root><root-step>G</root-step></root><kind>dominant</kind></harmony>
      <note>
        <pitch><step>E</step><octave>4</octave></pitch>
        <duration>2</duration><type>quarter</type>
        <notations><articulations><staccato/></articulations></notations>
      </note>
      <note><rest/><duration>2</duration><type>quarter</type></note>
    </measure>
  </part>
</score-partwise>`;

const compilerWasm =
  "https://cdn.jsdelivr.net/npm/@myriaddreamin/typst-ts-web-compiler/pkg/typst_ts_web_compiler_bg.wasm";
const rendererWasm =
  "https://cdn.jsdelivr.net/npm/@myriaddreamin/typst-ts-renderer/pkg/typst_ts_renderer_bg.wasm";
const themeStorageKey = "musicxml-to-scorify-theme";
const textDecoder = new TextDecoder();
const MUSICXML_TO_SCORIFY = "musicxml-to-scorify";
const SCORIFY_TO_MUSICXML = "scorify-to-musicxml";
const themeLabels = {
  system: "System",
  light: "Light",
  dark: "Dark",
};
const conversionLabels = {
  [MUSICXML_TO_SCORIFY]: {
    badge: "MusicXML -> Scorify",
    inputTitle: "MusicXML",
    outputTitle: "Typst",
    renderTitle: "Render generated Typst",
    downloadLabel: "Typst",
    downloadTitle: "Download Typst",
    downloadType: "text/plain;charset=utf-8",
    downloadName: "scorify-score.typ",
    fileAccept: ".musicxml,.xml,.mxl",
    filePrompt: "Choose MusicXML",
    fileHint: ".musicxml .xml .mxl",
    urlPlaceholder: "https://example.com/score.musicxml",
  },
  [SCORIFY_TO_MUSICXML]: {
    badge: "Scorify -> MusicXML",
    inputTitle: "Scorify",
    outputTitle: "MusicXML",
    renderTitle: "Render the Scorify source",
    downloadLabel: "MusicXML",
    downloadTitle: "Download MusicXML",
    downloadType: "application/vnd.recordare.musicxml+xml;charset=utf-8",
    downloadName: "score.musicxml",
    fileAccept: ".typ,.txt,.scorify",
    filePrompt: "Choose Scorify",
    fileHint: ".typ .txt .scorify",
    urlPlaceholder: "https://example.com/score.typ",
  },
};

function classNames(...values) {
  return values.filter(Boolean).join(" ");
}

function systemTheme() {
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function storedThemePreference() {
  const saved = localStorage.getItem(themeStorageKey);
  return saved === "light" || saved === "dark" ? saved : "system";
}

function resolvedTheme(preference) {
  return preference === "system" ? systemTheme() : preference;
}

function normalizeScorifyDocument(source) {
  const body = source
    .replace(/#import\s+(?:"@preview\/scorify:[^"]+"|"[^"]*lib\.typ")\s*:\s*score\s*\n*/g, "")
    .trim();

  return `#set page(margin: 16mm)
#import "/scorify/lib.typ": score

${body}
`;
}

function makeOptions(measuresPerLine) {
  const parsed = Number.parseInt(measuresPerLine, 10);
  return JSON.stringify({
    includeImport: false,
    includeComment: false,
    measuresPerLine: Number.isFinite(parsed) && parsed > 0 ? parsed : undefined,
  });
}

function defaultInputText(direction) {
  return direction === MUSICXML_TO_SCORIFY ? SAMPLE_XML : SAMPLE_TYPST;
}

function defaultOutputText(direction) {
  return direction === MUSICXML_TO_SCORIFY ? SAMPLE_TYPST : SAMPLE_XML;
}

function outputFilename(direction, file, url) {
  const fallback = direction === MUSICXML_TO_SCORIFY ? "scorify-score.typ" : "score.musicxml";
  const rawName = file?.name || url.trim().split("/").pop() || "";
  if (!rawName) return fallback;

  const stem = rawName.replace(/\.[^.]+$/, "") || "score";
  return direction === MUSICXML_TO_SCORIFY ? `${stem}.typ` : `${stem}.musicxml`;
}

function downloadBlob(filename, type, data) {
  const blob = data instanceof Blob ? data : new Blob([data], { type });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  URL.revokeObjectURL(url);
}

async function waitForTypst() {
  if (globalThis.$typst) return globalThis.$typst;

  const script = document.getElementById("typst");
  await new Promise((resolve, reject) => {
    script.addEventListener("load", resolve, { once: true });
    script.addEventListener("error", () => reject(new Error("Typst runtime failed to load")), {
      once: true,
    });
  });

  return globalThis.$typst;
}

async function configureTypstRuntime() {
  const $typst = await waitForTypst();

  $typst.setCompilerInitOptions({
    getModule: () => compilerWasm,
  });
  $typst.setRendererInitOptions({
    getModule: () => rendererWasm,
  });

  const [scorifyLib, scorifyWasm] = await Promise.all([
    fetch("./typst/scorify/lib.typ").then((response) => {
      if (!response.ok) throw new Error("Could not load web-converter/typst/scorify/lib.typ");
      return response.text();
    }),
    fetch("./typst/scorify/scorify_wasm.wasm").then((response) => {
      if (!response.ok) {
        throw new Error("Could not load web-converter/typst/scorify/scorify_wasm.wasm");
      }
      return response.arrayBuffer();
    }),
  ]);

  $typst.resetShadow();
  await $typst.addSource("/scorify/lib.typ", scorifyLib);
  $typst.mapShadow("/scorify/scorify_wasm.wasm", new Uint8Array(scorifyWasm));

  return $typst;
}

function App() {
  const [runtime, setRuntime] = useState(null);
  const [runtimeStatus, setRuntimeStatus] = useState("Loading");
  const [direction, setDirection] = useState(MUSICXML_TO_SCORIFY);
  const [sourceMode, setSourceMode] = useState("file");
  const [inputText, setInputText] = useState(SAMPLE_XML);
  const [url, setUrl] = useState("");
  const [file, setFile] = useState(null);
  const [measuresPerLine, setMeasuresPerLine] = useState("");
  const [outputText, setOutputText] = useState(SAMPLE_TYPST);
  const [previewDocument, setPreviewDocument] = useState("");
  const [previewHtml, setPreviewHtml] = useState("");
  const [status, setStatus] = useState("Ready");
  const [error, setError] = useState("");
  const [isConverting, setIsConverting] = useState(false);
  const [isRendering, setIsRendering] = useState(false);
  const [themePreference, setThemePreference] = useState(storedThemePreference);
  const [activeTheme, setActiveTheme] = useState(() => resolvedTheme(storedThemePreference()));
  const previewRef = useRef(null);
  const config = conversionLabels[direction];

  const canConvert = useMemo(() => {
    if (isConverting) return false;
    if (sourceMode === "file") return Boolean(file);
    if (sourceMode === "url") return Boolean(url.trim());
    return Boolean(inputText.trim());
  }, [file, inputText, isConverting, sourceMode, url]);

  const canRender = useMemo(() => {
    if (!runtime || isRendering) return false;
    if (direction === MUSICXML_TO_SCORIFY) {
      return Boolean(outputText.trim());
    }
    if (sourceMode === "paste") {
      return Boolean(inputText.trim());
    }
    return Boolean(previewDocument.trim());
  }, [direction, inputText, isRendering, outputText, previewDocument, runtime, sourceMode]);

  useEffect(() => {
    let alive = true;

    Promise.all([initConverter(), configureTypstRuntime()])
      .then(([, typstRuntime]) => {
        if (!alive) return;
        setRuntime(typstRuntime);
        setRuntimeStatus("Ready");
        setStatus("Ready");
      })
      .catch((err) => {
        if (!alive) return;
        setRuntimeStatus("Error");
        setError(err.message || String(err));
      });

    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    setSourceMode("file");
    setInputText(defaultInputText(direction));
    setOutputText(defaultOutputText(direction));
    setPreviewDocument(direction === MUSICXML_TO_SCORIFY ? defaultOutputText(direction) : defaultInputText(direction));
    setFile(null);
    setUrl("");
    setError("");
    setPreviewHtml("");
    setStatus("Ready");
  }, [direction]);

  useEffect(() => {
    const applyTheme = () => {
      const next = resolvedTheme(themePreference);
      setActiveTheme(next);
      document.documentElement.dataset.theme = next;
      document.documentElement.style.colorScheme = next;
    };

    applyTheme();

    if (themePreference === "system") {
      localStorage.removeItem(themeStorageKey);
    } else {
      localStorage.setItem(themeStorageKey, themePreference);
    }

    const media = window.matchMedia?.("(prefers-color-scheme: dark)");
    if (themePreference !== "system" || !media) return undefined;

    media.addEventListener("change", applyTheme);
    return () => media.removeEventListener("change", applyTheme);
  }, [themePreference]);

  useEffect(() => {
    if (!previewHtml || !previewRef.current) return;

    const svgs = previewRef.current.querySelectorAll("svg");
    svgs.forEach((svg) => {
      const width = Number.parseFloat(svg.getAttribute("width") || "0");
      const height = Number.parseFloat(svg.getAttribute("height") || "0");
      svg.removeAttribute("width");
      svg.removeAttribute("height");
      svg.style.width = "100%";
      svg.style.height = width > 0 && height > 0 ? "auto" : "";
    });
  }, [previewHtml]);

  async function loadSource() {
    if (sourceMode === "file") {
      const bytes = new Uint8Array(await file.arrayBuffer());
      return {
        bytes,
        filename: file.name,
        text: textDecoder.decode(bytes),
      };
    }

    if (sourceMode === "url") {
      const response = await fetch(url.trim());
      if (!response.ok) throw new Error(`Fetch failed: ${response.status}`);
      const bytes = new Uint8Array(await response.arrayBuffer());
      const filename = new URL(url.trim(), window.location.href).pathname.split("/").pop() ||
        (direction === MUSICXML_TO_SCORIFY ? "score.musicxml" : "score.typ");
      return {
        bytes,
        filename,
        text: textDecoder.decode(bytes),
      };
    }

    return {
      bytes: new TextEncoder().encode(inputText),
      filename: direction === MUSICXML_TO_SCORIFY ? "score.musicxml" : "score.typ",
      text: inputText,
    };
  }

  function currentPreviewDocument() {
    if (direction === MUSICXML_TO_SCORIFY) {
      return outputText.trim() ? outputText : "";
    }
    if (sourceMode === "paste") {
      return inputText.trim() ? normalizeScorifyDocument(inputText) : "";
    }
    return previewDocument;
  }

  async function convert() {
    if (!canConvert) return;

    setIsConverting(true);
    setError("");
    setStatus("Converting");

    try {
      const source = await loadSource();

      if (direction === MUSICXML_TO_SCORIFY) {
        const options = makeOptions(measuresPerLine);
        const scoreCall = sourceMode === "paste"
          ? convert_musicxml_text_with_options_wasm(source.text, options)
          : convert_musicxml_file_wasm(source.bytes, source.filename, options);
        const document = normalizeScorifyDocument(scoreCall);

        setOutputText(document);
        setPreviewDocument(document);
        setStatus("Converted");
        await renderPreview(document);
      } else {
        const xml = sourceMode === "paste"
          ? convert_scorify_text_wasm(source.text)
          : convert_scorify_file_wasm(source.bytes);
        const document = normalizeScorifyDocument(source.text);

        setOutputText(xml);
        setPreviewDocument(document);
        setStatus("Converted");
        await renderPreview(document);
      }
    } catch (err) {
      setStatus("Error");
      setError(err.message || String(err));
    } finally {
      setIsConverting(false);
    }
  }

  async function renderPreview(code = currentPreviewDocument()) {
    if (!runtime || !code) return;

    setIsRendering(true);
    setError("");
    setStatus("Rendering");

    try {
      const svg = await runtime.svg({ mainContent: code });
      setPreviewHtml(svg);
      setStatus("Rendered");
    } catch (err) {
      setStatus("Error");
      setError(err.message || String(err));
    } finally {
      setIsRendering(false);
    }
  }

  async function exportPdf() {
    const document = currentPreviewDocument();
    if (!runtime || !document) return;

    setIsRendering(true);
    setError("");
    setStatus("Exporting");

    try {
      const pdf = await runtime.pdf({ mainContent: document });
      downloadBlob("scorify-score.pdf", "application/pdf", pdf);
      setStatus("Exported");
    } catch (err) {
      setStatus("Error");
      setError(err.message || String(err));
    } finally {
      setIsRendering(false);
    }
  }

  function cycleTheme() {
    setThemePreference((current) => {
      if (current === "system") return "light";
      if (current === "light") return "dark";
      return "system";
    });
  }

  return h("main", { class: "shell" }, [
    h("header", { class: "topbar" }, [
      h("div", { class: "brand" }, [
        h("div", { class: "mark" }, "S"),
        h("div", null, [
          h("h1", null, "MusicXML and Scorify"),
          h("div", { class: "subline" }, "Typst preview powered by local Scorify"),
        ]),
      ]),
      h("div", { class: "topActions" }, [
        h("div", { class: "segments twoUp modeSwitch", role: "tablist" }, [
          [MUSICXML_TO_SCORIFY, "MusicXML -> Scorify"],
          [SCORIFY_TO_MUSICXML, "Scorify -> MusicXML"],
        ].map(([mode, label]) =>
          h(
            "button",
            {
              class: classNames(direction === mode && "active"),
              onClick: () => setDirection(mode),
              type: "button",
            },
            label,
          ),
        )),
        h("span", { class: classNames("pill", runtimeStatus === "Ready" && "good") }, runtimeStatus),
        h(
          "button",
          {
            class: "themeToggle",
            onClick: cycleTheme,
            title: `Theme: ${themeLabels[themePreference]} (${activeTheme})`,
            type: "button",
          },
          [
            h("span", { "aria-hidden": "true" }, activeTheme === "dark" ? "Moon" : "Sun"),
            h("span", null, themeLabels[themePreference]),
          ],
        ),
        h(
          "button",
          {
            class: "ghost",
            onClick: () => renderPreview(),
            disabled: !canRender,
            title: config.renderTitle,
          },
          "Render",
        ),
        h(
          "button",
          {
            class: "ghost",
            onClick: () =>
              downloadBlob(outputFilename(direction, file, url), config.downloadType, outputText),
            title: config.downloadTitle,
          },
          config.downloadLabel,
        ),
        h(
          "button",
          {
            class: "primary",
            onClick: exportPdf,
            disabled: !canRender,
            title: "Export PDF",
          },
          "PDF",
        ),
      ]),
    ]),

    h("section", { class: "workspace" }, [
      h("aside", { class: "panel inputPanel" }, [
        h("div", { class: "panelHead" }, [
          h("h2", null, config.inputTitle),
          h("span", { class: "status" }, status),
        ]),
        h("div", { class: "segments", role: "tablist" }, [
          ["file", "File"],
          ["paste", "Paste"],
          ["url", "URL"],
        ].map(([mode, label]) =>
          h(
            "button",
            {
              class: classNames(sourceMode === mode && "active"),
              onClick: () => setSourceMode(mode),
              type: "button",
            },
            label,
          ),
        )),

        sourceMode === "file" &&
          h("label", { class: classNames("dropzone", file && "hasFile") }, [
            h("input", {
              type: "file",
              accept: config.fileAccept,
              onChange: (event) => setFile(event.currentTarget.files?.[0] || null),
            }),
            h("span", { class: "dropTitle" }, file ? file.name : config.filePrompt),
            h("span", { class: "dropMeta" }, file ? `${Math.ceil(file.size / 1024)} KB` : config.fileHint),
          ]),

        sourceMode === "paste" &&
          h("textarea", {
            class: "xmlBox",
            spellcheck: false,
            value: inputText,
            onInput: (event) => setInputText(event.currentTarget.value),
          }),

        sourceMode === "url" &&
          h("input", {
            class: "urlInput",
            type: "url",
            value: url,
            placeholder: config.urlPlaceholder,
            onInput: (event) => setUrl(event.currentTarget.value),
          }),

        direction === MUSICXML_TO_SCORIFY &&
          h("div", { class: "optionGrid" }, [
            h("label", null, [
              h("span", null, "Measures"),
              h("input", {
                type: "number",
                min: "1",
                max: "16",
                value: measuresPerLine,
                onInput: (event) => setMeasuresPerLine(event.currentTarget.value),
              }),
            ]),
          ]),

        h(
          "button",
          {
            class: "convertButton",
            onClick: convert,
            disabled: !canConvert || runtimeStatus !== "Ready",
          },
          isConverting ? "Converting" : "Convert",
        ),

        error && h("pre", { class: "errorBox" }, error),
      ]),

      h("section", { class: "panel codePanel" }, [
        h("div", { class: "panelHead" }, [
          h("h2", null, config.outputTitle),
          h("span", { class: "mini" }, `${outputText.length.toLocaleString()} chars`),
        ]),
        h("textarea", {
          class: "codeBox",
          spellcheck: false,
          value: outputText,
          onInput: (event) => setOutputText(event.currentTarget.value),
        }),
      ]),

      h("section", { class: "panel previewPanel" }, [
        h("div", { class: "panelHead" }, [
          h("h2", null, "Preview"),
          h("span", { class: classNames("mini", isRendering && "busy") }, isRendering ? "Rendering" : "SVG"),
        ]),
        h(
          "div",
          {
            ref: previewRef,
            class: classNames("previewStage", !previewHtml && "empty"),
            dangerouslySetInnerHTML: {
              __html: previewHtml || "<div class=\"emptyPreview\">No preview</div>",
            },
          },
        ),
      ]),
    ]),
  ]);
}

render(h(App), document.getElementById("app"));
