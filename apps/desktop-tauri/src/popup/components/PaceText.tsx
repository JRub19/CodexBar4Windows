import { buildPaceText, type PaceTextInput } from "../format/pace";

// Phase 3 D7: thin presentation wrapper over `buildPaceText`. We keep
// the formatting logic in a pure module so it stays under test, and the
// component only positions the two captions inside the metric row.

interface Props {
  input: PaceTextInput;
}

export function PaceText({ input }: Props) {
  const { left, right } = buildPaceText(input);
  if (left == null && right == null) return null;
  return (
    <div className="pace-text" aria-live="polite">
      {left ? <span className="pace-text__left">{left}</span> : <span />}
      {right ? <span className="pace-text__right">{right}</span> : null}
    </div>
  );
}
