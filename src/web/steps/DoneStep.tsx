// After the download: how to unpack the ZIP, and the readiness checks the
// desktop's Ready screen would show, with what is still left to do.

import type { ReadinessCheck } from "../protocol";

interface Props {
  kind: "new" | "existing";
  removal: boolean;
  layout: "overlay" | "full_copy";
  projectId: string;
  folderName: string | null;
  checks: ReadinessCheck[];
  onDownloadAgain: () => void;
  onLauncher: (() => void) | null;
}

const SYMBOL: Record<ReadinessCheck["state"], string> = { pass: "✓", action: "!", info: "i" };
const STATUS: Record<ReadinessCheck["state"], string> = { pass: "status-pass", action: "status-review", info: "status-info" };
const WORD: Record<ReadinessCheck["state"], string> = { pass: "Ready", action: "To do", info: "Note" };

export default function DoneStep({ kind, removal, layout, projectId, folderName, checks, onDownloadAgain, onLauncher }: Props) {
  return (
    <div className="stack">
      <section className="panel form-panel web-done">
        {kind === "new" ? (
          <ol>
            <li>Unpack the ZIP into <span className="mono">Documents/Paradox Interactive/Hearts of Iron IV/mod/</span>.</li>
            <li>It contains the <span className="mono">{projectId}</span> folder and <span className="mono">{projectId}.mod</span>; keep both there.</li>
            <li>Open the folder in your coding client.</li>
          </ol>
        ) : removal ? (
          <ol>
            <li>Back up your mod folder.</li>
            {layout === "overlay" ? <li>Delete the files listed in <span className="mono">HOI4_MOD_SETUP_CHANGES.md</span>, including <span className="mono">.hoi4-mod-setup/install.lock.json</span>.</li> : <li>Replace your mod folder with the folder in the ZIP after comparing them.</li>}
          </ol>
        ) : layout === "overlay" ? (
          <ol>
            <li>Back up your mod folder.</li>
            <li>Unpack the ZIP over it. <span className="mono">HOI4_MOD_SETUP_CHANGES.md</span> lists every file, including any to delete.</li>
            <li>Open the folder in your coding client.</li>
          </ol>
        ) : (
          <ol>
            <li>Unpack the ZIP next to your mod folder and compare before replacing it.</li>
            <li>Open the folder in your coding client.</li>
          </ol>
        )}
        {!removal && <p className="muted">The installation lock in <span className="mono">.hoi4-mod-setup/</span> records the exact source commit and every file hash, so the web edition or the desktop app can update or repair this setup later.</p>}
        <div className="button-row">
          {onLauncher && folderName && <button type="button" className="button" onClick={onLauncher}>Download launcher file</button>}
          <button type="button" className="button" onClick={onDownloadAgain}>Download again</button>
        </div>
      </section>
      {checks.length > 0 && (
        <section className="panel" aria-labelledby="checks-title">
          <h2 id="checks-title" className="panel-title">Checks</h2>
          <ul className="web-checks">
            {checks.map((check) => (
              <li key={check.id} className="check-row web-check">
                <span className={`status ${STATUS[check.state]} web-check-state`}><span className="status-symbol" aria-hidden="true">{SYMBOL[check.state]}</span> {WORD[check.state]}</span>
                <span><strong>{check.label}</strong><span className="muted web-check-detail"> {check.detail}</span></span>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
