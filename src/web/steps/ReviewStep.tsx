// The change review: what the ZIP adds, updates, restores, and removes, and
// a decision for every file the user changed. Merges are previewed before
// they can be chosen.

import type { Comparison, Decision, ExportEntry } from "../protocol";

interface Props {
  entries: ExportEntry[];
  decisions: Record<string, Decision>;
  onDecision: (path: string, decision: Decision) => void;
  merged: Record<string, string>;
  mergeErrors: Record<string, string>;
  basesReady: boolean;
  onMerge: (path: string) => void;
  onCompare: (path: string) => void;
  comparison: Comparison | null;
  onCloseComparison: () => void;
  existing: boolean;
  removal: boolean;
  layout: "overlay" | "full_copy";
  onLayout: (layout: "overlay" | "full_copy") => void;
  busy: boolean;
}

const GROUPS: Array<{ action: ExportEntry["action"]; title: string; note?: string }> = [
  { action: "create", title: "New" },
  { action: "update", title: "Updated", note: "You never changed these, so they get the new version." },
  { action: "restore", title: "Restored", note: "These setup files were missing from the folder." },
  { action: "remove", title: "To delete", note: "The setup no longer uses these and you never changed them. The ZIP's change list names them." },
  { action: "release", title: "No longer managed", note: "The setup no longer uses these, but you changed them, so they stay yours." },
  { action: "modified", title: "Kept as you changed them", note: "There is no newer version of these, so your changes stay." },
  { action: "unchanged", title: "Already up to date" },
];

const CHOICES: Array<{ value: Decision; label: string }> = [
  { value: "keep", label: "Keep mine" },
  { value: "replace", label: "Use incoming" },
  { value: "merge", label: "Merge" },
  { value: "rename", label: "Add incoming beside mine" },
  { value: "skip", label: "Skip" },
];

export default function ReviewStep(props: Props) {
  const { entries, decisions, onDecision, merged, mergeErrors, basesReady, onMerge, onCompare, comparison, onCloseComparison, existing, removal, layout, onLayout, busy } = props;
  const conflicts = entries.filter((entry) => entry.action === "conflict");
  const count = (action: ExportEntry["action"]) => entries.filter((entry) => entry.action === action).length;
  return (
    <div className="stack">
      <section className="panel">
        <ul className="web-counts">
          {GROUPS.filter((group) => count(group.action) > 0 && (existing || group.action !== "unchanged")).map((group) => (
            <li key={group.action}><strong>{count(group.action)}</strong> {group.title.toLowerCase()}</li>
          ))}
          {conflicts.length > 0 && <li><strong>{conflicts.length}</strong> need a decision</li>}
        </ul>
        {conflicts.map((entry) => {
          const choices = CHOICES.filter((choice) => choice.value !== "merge" || entry.mergeable);
          const decision = decisions[entry.path];
          return (
            <div key={entry.path} className="change-row web-conflict">
              <span className="mono web-path">{entry.path}</span>
              <fieldset className="web-decision">
                <legend className="visually-hidden">{entry.path}</legend>
                {choices.map((choice) => (
                  <label key={choice.value}>
                    <input
                      type="radio"
                      name={`decision-${entry.path}`}
                      checked={decision === choice.value}
                      disabled={choice.value === "merge" && !merged[entry.path]}
                      onChange={() => onDecision(entry.path, choice.value)}
                    /> {choice.label}
                  </label>
                ))}
                <button type="button" className="text-button" onClick={() => onCompare(entry.path)}>Compare</button>
                {entry.mergeable && !merged[entry.path] && (
                  <button type="button" className="text-button" disabled={busy || !basesReady} onClick={() => onMerge(entry.path)}>Preview merge</button>
                )}
              </fieldset>
              {mergeErrors[entry.path] && <p className="web-merge-error status-review"><span className="status-symbol" aria-hidden="true">!</span> {mergeErrors[entry.path]}</p>}
            </div>
          );
        })}
        {comparison && <ComparisonView comparison={comparison} onClose={onCloseComparison} />}
        {GROUPS.filter((group) => group.action !== "create" && group.action !== "unchanged" && count(group.action) > 0).map((group) => (
          <details key={group.action} className="manifest-details">
            <summary className="details">{group.title} ({count(group.action)})</summary>
            {group.note && <p className="muted web-group-note">{group.note}</p>}
            <ul className="web-file-list">
              {entries.filter((entry) => entry.action === group.action).map((entry) => <li key={entry.path}><span className="mono">{entry.path}</span></li>)}
            </ul>
          </details>
        ))}
        <details className="manifest-details">
          <summary className="details">All files</summary>
          <ul className="web-file-list">
            {entries.map((entry) => <li key={entry.path}><span className="mono">{entry.path}</span> <span className="muted">{entry.action} · {formatBytes(entry.size)}</span></li>)}
          </ul>
        </details>
      </section>
      {existing && (
        <fieldset className="panel update-source-options">
          <legend className="field-label">ZIP contents</legend>
          <label className="radio-row"><input type="radio" name="layout" checked={layout === "overlay"} onChange={() => onLayout("overlay")} /> {removal ? "Only the change list, naming the files to delete" : "Only new and changed files, to copy over your folder"}</label>
          <label className="radio-row"><input type="radio" name="layout" checked={layout === "full_copy"} onChange={() => onLayout("full_copy")} /> {removal ? "A full copy of the mod without the setup files" : "A full copy of the mod with the setup included"}</label>
        </fieldset>
      )}
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function ComparisonView({ comparison, onClose }: { comparison: Comparison; onClose: () => void }) {
  return (
    <section className="web-compare" aria-label={`Comparison of ${comparison.path}`}>
      <div className="proposal-heading">
        <strong className="mono">{comparison.path}</strong>
        <button type="button" className="text-button" onClick={onClose}>Close</button>
      </div>
      {comparison.binary ? <p className="muted">This file is binary and cannot be shown.</p> : (
        <div className={`web-compare-grid${comparison.merged ? " three" : ""}`}>
          <div><div className="field-label">Yours</div><pre>{comparison.local ?? ""}</pre></div>
          <div><div className="field-label">Incoming</div><pre>{comparison.incoming ?? ""}</pre></div>
          {comparison.merged && <div><div className="field-label">Merged</div><pre>{comparison.merged}</pre></div>}
        </div>
      )}
    </section>
  );
}
