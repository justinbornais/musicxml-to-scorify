import { h, render } from "https://esm.sh/preact@10.27.2";
import { useEffect, useMemo, useRef, useState } from "https://esm.sh/preact@10.27.2/hooks";
import initConverter, {
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
const themeLabels = {
  system: "System",
  light: "Light",
  dark: "Dark",
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

function typstDocument(scoreCall) {
  const body = scoreCall
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
  const [sourceMode, setSourceMode] = useState("file");
  const [xmlText, setXmlText] = useState(SAMPLE_XML);
  const [url, setUrl] = useState("");
  const [file, setFile] = useState(null);
  const [measuresPerLine, setMeasuresPerLine] = useState("");
  const [typstCode, setTypstCode] = useState(SAMPLE_TYPST);
  const [previewHtml, setPreviewHtml] = useState("");
  const [status, setStatus] = useState("Ready");
  const [error, setError] = useState("");
  const [isConverting, setIsConverting] = useState(false);
  const [isRendering, setIsRendering] = useState(false);
  const [themePreference, setThemePreference] = useState(storedThemePreference);
  const [activeTheme, setActiveTheme] = useState(() => resolvedTheme(storedThemePreference()));
  const previewRef = useRef(null);

  const canConvert = useMemo(() => {
    if (isConverting) return false;
    if (sourceMode === "file") return Boolean(file);
    if (sourceMode === "url") return Boolean(url.trim());
    return Boolean(xmlText.trim());
  }, [file, isConverting, sourceMode, url, xmlText]);

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

  async function convert() {
    if (!canConvert) return;

    setIsConverting(true);
    setError("");
    setStatus("Converting");

    try {
      const options = makeOptions(measuresPerLine);
      let scoreCall;

      if (sourceMode === "file") {
        const bytes = new Uint8Array(await file.arrayBuffer());
        scoreCall = convert_musicxml_file_wasm(bytes, file.name, options);
      } else if (sourceMode === "url") {
        const response = await fetch(url.trim());
        if (!response.ok) throw new Error(`Fetch failed: ${response.status}`);
        const bytes = new Uint8Array(await response.arrayBuffer());
        const name = new URL(url.trim(), window.location.href).pathname.split("/").pop() || "score.musicxml";
        scoreCall = convert_musicxml_file_wasm(bytes, name, options);
      } else {
        scoreCall = convert_musicxml_text_with_options_wasm(xmlText, options);
      }

      const document = typstDocument(scoreCall);
      setTypstCode(document);
      setStatus("Converted");
      await renderPreview(document);
    } catch (err) {
      setStatus("Error");
      setError(err.message || String(err));
    } finally {
      setIsConverting(false);
    }
  }

  async function renderPreview(code = typstCode) {
    if (!runtime) return;

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
    if (!runtime) return;

    setIsRendering(true);
    setError("");
    setStatus("Exporting");

    try {
      const pdf = await runtime.pdf({ mainContent: typstCode });
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
          h("h1", null, "MusicXML to Scorify"),
          h("div", { class: "subline" }, "Typst preview powered by local Scorify"),
        ]),
      ]),
      h("div", { class: "topActions" }, [
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
            disabled: !runtime || isRendering,
            title: "Render preview",
          },
          "Render",
        ),
        h(
          "button",
          {
            class: "ghost",
            onClick: () => downloadBlob("scorify-score.typ", "text/plain;charset=utf-8", typstCode),
            title: "Download Typst",
          },
          "Typst",
        ),
        h(
          "button",
          {
            class: "primary",
            onClick: exportPdf,
            disabled: !runtime || isRendering,
            title: "Export PDF",
          },
          "PDF",
        ),
      ]),
    ]),

    h("section", { class: "workspace" }, [
      h("aside", { class: "panel inputPanel" }, [
        h("div", { class: "panelHead" }, [
          h("h2", null, "MusicXML"),
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
              accept: ".musicxml,.xml,.mxl",
              onChange: (event) => setFile(event.currentTarget.files?.[0] || null),
            }),
            h("span", { class: "dropTitle" }, file ? file.name : "Choose MusicXML"),
            h("span", { class: "dropMeta" }, file ? `${Math.ceil(file.size / 1024)} KB` : ".musicxml .xml .mxl"),
          ]),

        sourceMode === "paste" &&
          h("textarea", {
            class: "xmlBox",
            spellcheck: false,
            value: xmlText,
            onInput: (event) => setXmlText(event.currentTarget.value),
          }),

        sourceMode === "url" &&
          h("input", {
            class: "urlInput",
            type: "url",
            value: url,
            placeholder: "https://example.com/score.musicxml",
            onInput: (event) => setUrl(event.currentTarget.value),
          }),

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
          h("h2", null, "Typst"),
          h("span", { class: "mini" }, `${typstCode.length.toLocaleString()} chars`),
        ]),
        h("textarea", {
          class: "codeBox",
          spellcheck: false,
          value: typstCode,
          onInput: (event) => setTypstCode(event.currentTarget.value),
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
