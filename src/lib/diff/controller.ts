import type { AppError, DiffDocument, DiffTarget } from "../ipc/types";

export interface DiffSelection {
  repoId: string;
  path: string;
  target: DiffTarget;
}

export interface DiffState {
  selection: DiffSelection | null;
  doc: DiffDocument | null;
  loading: boolean;
  error: AppError | null;
}

export function emptyDiffState(): DiffState {
  return { selection: null, doc: null, loading: false, error: null };
}

/** Only the latest open/retry may update the panel; closing invalidates reads. */
export class DiffController {
  private request = 0;
  private state = emptyDiffState();

  constructor(
    private read: (repoId: string, target: DiffTarget) => Promise<DiffDocument>,
    private onChange: (state: DiffState) => void
  ) {}

  private publish(state: DiffState): void {
    this.state = state;
    this.onChange(state);
  }

  async open(selection: DiffSelection): Promise<void> {
    const request = ++this.request;
    this.publish({ selection, doc: null, loading: true, error: null });
    try {
      const doc = await this.read(selection.repoId, selection.target);
      if (request !== this.request) return;
      this.publish({ selection, doc, loading: false, error: null });
    } catch (error) {
      if (request !== this.request) return;
      this.publish({ selection, doc: null, loading: false, error: error as AppError });
    }
  }

  retry(): Promise<void> {
    return this.state.selection ? this.open(this.state.selection) : Promise.resolve();
  }

  close(): void {
    this.request++;
    this.publish(emptyDiffState());
  }
}
