// The magic wand on the description step: the selected setup assistant turns
// a rough note into a fuller description. The draft replaces the text only
// after it arrives, is labelled as a suggestion, and can be undone; the user
// still reviews and edits it like anything they typed.

import { useEffect, useRef, useState } from "react";

export interface DraftResult {
  value: string | null;
  error?: string;
}

interface Props {
  description: string;
  assistantName: string;
  onApply: (text: string) => void;
  draft: (note: string) => Promise<DraftResult>;
  disabled?: boolean;
}

export const MAX_NOTE_CHARS = 4000;

export default function DescriptionWand({ description, assistantName, onApply, draft, disabled }: Props) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [previous, setPrevious] = useState<string | null>(null);
  const applied = useRef<string | null>(null);
  const live = useRef(true);
  useEffect(() => () => { live.current = false; }, []);
  // Once the user edits the draft, Undo would discard their edits.
  useEffect(() => {
    if (applied.current !== null && description !== applied.current) {
      applied.current = null;
      setPrevious(null);
    }
  }, [description]);
  const note = description.trim();
  const tooLong = note.length > MAX_NOTE_CHARS;
  const expand = async () => {
    setPending(true);
    setError(null);
    const result = await draft(note).catch((): DraftResult => ({ value: null, error: "The draft could not be written." }));
    if (!live.current) return;
    setPending(false);
    if (!result.value) {
      setError(result.error ?? "The draft could not be written. Your text is unchanged.");
      return;
    }
    setPrevious(description);
    applied.current = result.value;
    onApply(result.value);
  };
  const undo = () => {
    if (previous === null) return;
    applied.current = null;
    onApply(previous);
    setPrevious(null);
  };
  return (
    <div className="description-wand">
      <div className="description-wand-row">
        <button type="button" className="button secondary wand-button" onClick={() => void expand()} disabled={disabled || pending || !note || tooLong} aria-busy={pending}>
          <svg aria-hidden="true" viewBox="0 0 20 20" width="16" height="16" className="wand-icon"><path d="M3 17 13 7m1-4v2m0 4v2m-3-5h2m4 0h2M16.5 3.5l-1 1m-3 3-1 1m4.5 0 1 1" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" /></svg>
          {pending ? "Writing…" : "Expand description"}
        </button>
        {previous !== null && <button type="button" className="text-button" onClick={undo}>Undo</button>}
      </div>
      {tooLong && <p className="muted" role="status">Shorten the note to {MAX_NOTE_CHARS} characters to expand it.</p>}
      {previous !== null && <p className="muted" role="status">Suggested by {assistantName}. Review and edit it before you continue.</p>}
      {!previous && !error && !tooLong && <p className="muted wand-help">Type a rough idea, then let {assistantName} make it more thorough.</p>}
      {error && <p className="callout review" role="alert">{error}</p>}
    </div>
  );
}
