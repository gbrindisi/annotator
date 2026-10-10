import { autoGrow, el } from './dom';

export type ComposerOptions = {
  placeholder: string;
  submitLabel: string;
  initialValue?: string;
  /** Called with the trimmed text. Return an error message to keep the composer open. */
  onSubmit: (text: string) => string | void;
  onCancel?: () => void;
  autofocus?: boolean;
  compact?: boolean;
};

export type Composer = {
  el: HTMLElement;
  textarea: HTMLTextAreaElement;
  focus: () => void;
  setError: (message: string | null) => void;
};

/** Textarea with Enter to submit, Shift-Enter for a newline and Esc to cancel. */
export const buildComposer = (options: ComposerOptions): Composer => {
  const textarea = el('textarea', {
    className: 'mc-textarea',
    placeholder: options.placeholder,
    rows: '1',
    spellcheck: 'true',
  });
  textarea.value = options.initialValue ?? '';
  const error = el('div', { className: 'mc-error', hidden: true });
  const submit = el('button', { className: 'mc-btn mc-btn-primary', type: 'button', text: options.submitLabel });
  const cancel = el('button', { className: 'mc-btn', type: 'button', text: 'Cancel' });
  const actions = el('div', { className: 'mc-composer-actions' }, [cancel, submit]);
  const root = el('div', { className: `mc-composer${options.compact ? ' is-compact' : ''}` }, [textarea, error, actions]);

  const setError = (message: string | null) => {
    error.textContent = message ?? '';
    error.hidden = !message;
  };

  const doSubmit = () => {
    const text = textarea.value.trim();
    if (!text) {
      options.onCancel?.();
      return;
    }
    const result = options.onSubmit(text);
    if (typeof result === 'string') setError(result);
  };

  submit.addEventListener('click', doSubmit);
  cancel.addEventListener('click', () => options.onCancel?.());
  textarea.addEventListener('input', () => {
    autoGrow(textarea);
    setError(null);
  });
  textarea.addEventListener('keydown', event => {
    if (event.key === 'Enter' && !event.shiftKey && !event.altKey) {
      event.preventDefault();
      doSubmit();
    } else if (event.key === 'Escape') {
      event.preventDefault();
      options.onCancel?.();
    }
  });

  const focus = () => {
    textarea.focus();
    textarea.setSelectionRange(textarea.value.length, textarea.value.length);
    autoGrow(textarea);
  };
  if (options.autofocus) window.setTimeout(focus, 0);

  return { el: root, textarea, focus, setError };
};
