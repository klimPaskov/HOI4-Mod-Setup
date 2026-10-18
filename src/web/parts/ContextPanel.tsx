// Optional context while describing a mod: documents the assistant may read
// as redacted excerpts, and images for the thumbnail or visual direction.
// Images never reach a provider.

import type { ChangeEvent } from "react";
import type { ContextDocument } from "../protocol";

export interface ContextDocumentEntry {
  document: ContextDocument;
  /** Base64 of the file, kept only to copy it into the project. */
  data: string;
  include: boolean;
  keepCopy: boolean;
}

export interface ContextImageEntry {
  id: string;
  name: string;
  dataUrl: string;
  /** A bounded JPEG rendering for the visual-direction copy. */
  reference: string | null;
  keep: boolean;
}

interface Props {
  documents: ContextDocumentEntry[];
  images: ContextImageEntry[];
  refused: string[];
  onAddDocuments: (files: File[]) => void;
  onAddImages: (files: File[]) => void;
  onDocument: (id: string, change: Partial<ContextDocumentEntry>) => void;
  onImage: (id: string, change: Partial<ContextImageEntry>) => void;
  onRemoveDocument: (id: string) => void;
  onRemoveImage: (id: string) => void;
  onThumbnail: (image: ContextImageEntry) => void;
  assistantName: string;
  disabled: boolean;
}

const DOCUMENT_TYPES = ".md,.markdown,.txt,.csv,.json,.yml,.yaml,.toml,.lua,.py,.js,.ts,.gui,.gfx,.asset,.mod,.html,.xml";

export default function ContextPanel(props: Props) {
  const { documents, images, refused, onAddDocuments, onAddImages, onDocument, onImage, onRemoveDocument, onRemoveImage, onThumbnail, assistantName, disabled } = props;
  const pick = (handler: (files: File[]) => void) => (event: ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(event.target.files ?? []);
    event.target.value = "";
    if (files.length > 0) handler(files);
  };
  return (
    <details className="panel web-context">
      <summary className="details web-context-summary">Add context (optional)</summary>
      <div className="web-context-body">
        <p className="muted">Documents help {assistantName || "the assistant"} describe the mod; it reads a redacted excerpt, never the file's location. Images stay in this tab: use one as the thumbnail or keep copies for visual direction.</p>
        <div className="button-row web-context-actions">
          <label className="button">
            Add documents
            <input type="file" multiple accept={DOCUMENT_TYPES} className="visually-hidden" disabled={disabled} onChange={pick(onAddDocuments)} />
          </label>
          <label className="button">
            Add images
            <input type="file" multiple accept="image/png,image/jpeg,image/webp" className="visually-hidden" disabled={disabled} onChange={pick(onAddImages)} />
          </label>
        </div>
        {refused.length > 0 && <ul className="web-refused status-review">{refused.map((reason) => <li key={reason}>{reason}</li>)}</ul>}
        {documents.length > 0 && (
          <ul className="web-attachments" aria-label="Documents">
            {documents.map(({ document, include, keepCopy }) => (
              <li key={document.id} className="web-attachment">
                <span className="mono web-path">{document.name}</span>
                {document.redacted && <span className="status status-review">Some lines were left out</span>}
                {document.truncated && <span className="status status-info">Long; the start is used</span>}
                <label className="check-row web-inline"><input type="checkbox" checked={include} onChange={(event) => onDocument(document.id, { include: event.target.checked })} /> Send to {assistantName || "the assistant"}</label>
                <label className="check-row web-inline"><input type="checkbox" checked={keepCopy} disabled={!document.copyable} onChange={(event) => onDocument(document.id, { keepCopy: event.target.checked })} /> Keep a copy in docs/reference</label>
                <button type="button" className="text-button" onClick={() => onRemoveDocument(document.id)}>Remove</button>
              </li>
            ))}
          </ul>
        )}
        {images.length > 0 && (
          <ul className="web-attachments" aria-label="Images">
            {images.map((image) => (
              <li key={image.id} className="web-attachment">
                <img src={image.dataUrl} alt="" width={48} height={48} className="web-attachment-preview" />
                <span className="mono web-path">{image.name}</span>
                <button type="button" className="text-button" onClick={() => onThumbnail(image)}>Use as thumbnail</button>
                <label className="check-row web-inline"><input type="checkbox" checked={image.keep} disabled={!image.reference} onChange={(event) => onImage(image.id, { keep: event.target.checked })} /> Keep in docs/visual_direction</label>
                <button type="button" className="text-button" onClick={() => onRemoveImage(image.id)}>Remove</button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </details>
  );
}
