// The scan findings the setup assistant may read. Each can be left out
// before anything is sent; detected values the files decide are marked.

import type { ScanFinding } from "../protocol";

interface Props {
  findings: ScanFinding[];
  excluded: Set<string>;
  onToggle: (id: string, include: boolean) => void;
  assistantName: string;
}

function summarize(value: unknown): string {
  const text = typeof value === "string" ? value : JSON.stringify(value) ?? "";
  return text.length > 140 ? `${text.slice(0, 140)}…` : text;
}

function label(finding: ScanFinding): string {
  const key = finding.key ?? finding.id;
  return key.replace(/[._]/g, " ").replace(/^\w/, (letter) => letter.toUpperCase());
}

export default function FindingsPanel({ findings, excluded, onToggle, assistantName }: Props) {
  if (findings.length === 0) return null;
  const included = findings.filter((finding) => !excluded.has(finding.id)).length;
  return (
    <details className="web-findings">
      <summary className="details">What {assistantName || "the assistant"} will read ({included} of {findings.length} findings)</summary>
      <p className="muted">These are summaries from the scan, not your files. Leave out anything you do not want to send.</p>
      <ul className="web-finding-list">
        {findings.map((finding) => (
          <li key={finding.id} className="check-row web-finding">
            <label className="web-inline">
              <input type="checkbox" checked={!excluded.has(finding.id)} onChange={(event) => onToggle(finding.id, event.target.checked)} />
              <strong>{label(finding)}</strong>
            </label>
            <span className="muted mono web-finding-value">{summarize(finding.value)}</span>
            {finding.path && <span className="muted web-finding-path">{finding.path}</span>}
          </li>
        ))}
      </ul>
    </details>
  );
}
