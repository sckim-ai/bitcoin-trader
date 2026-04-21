import {
  type ChangeEventHandler,
  type FocusEventHandler,
  type InputHTMLAttributes,
  useState,
} from "react";
import { Input } from "./Input";

// Controlled numeric input that decouples the parent's `number` state from
// the raw text the user is typing. Without this separation, binding a number
// directly to `<input type="number">` causes two classic bugs in React:
//   1) Backspacing to clear the value produces "" which `Number()` collapses
//      back to 0 — the parent state never changes, so React re-renders the
//      field as "0" and the field appears un-clearable.
//   2) Typing in front of an existing "0" can produce "020" because the DOM
//      value diverges from the coerced number prop.
// We hold the text locally, forward a number to the parent only when the
// buffer parses cleanly, and clamp to `min` (or 0) on blur.
interface NumberInputProps
  extends Omit<InputHTMLAttributes<HTMLInputElement>, "value" | "onChange" | "type"> {
  label?: string;
  value: number;
  onValueChange: (value: number) => void;
}

const isIntermediate = (s: string) => s === "" || s === "-" || s === "." || s === "-.";

export function NumberInput({
  value,
  onValueChange,
  onBlur,
  min,
  ...rest
}: NumberInputProps) {
  const [text, setText] = useState<string>(String(value));
  const [lastSyncedValue, setLastSyncedValue] = useState<number>(value);

  // Re-sync when the parent resets or patches the value externally (e.g. a
  // Reset button). Updating state during render is the React-recommended
  // alternative to useEffect for deriving state from props — it avoids an
  // extra render and the `react-hooks/set-state-in-effect` lint rule.
  // Guard against clobbering intermediate text whose numeric interpretation
  // already equals the prop.
  if (value !== lastSyncedValue) {
    setLastSyncedValue(value);
    if (Number(text) !== value) setText(String(value));
  }

  const handleChange: ChangeEventHandler<HTMLInputElement> = (e) => {
    const next = e.target.value;
    setText(next);
    if (isIntermediate(next)) return;
    const n = Number(next);
    if (!Number.isNaN(n)) onValueChange(n);
  };

  const handleBlur: FocusEventHandler<HTMLInputElement> = (e) => {
    if (isIntermediate(text) || Number.isNaN(Number(text))) {
      const fallback = typeof min === "number" ? min : 0;
      setText(String(fallback));
      onValueChange(fallback);
    }
    onBlur?.(e);
  };

  return (
    <Input
      {...rest}
      type="number"
      min={min}
      value={text}
      onChange={handleChange}
      onBlur={handleBlur}
    />
  );
}
