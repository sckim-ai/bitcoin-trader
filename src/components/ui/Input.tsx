import { type InputHTMLAttributes, type MouseEvent, useState } from "react";
import { Eye, EyeOff } from "lucide-react";

interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  label?: string;
  passwordToggle?: boolean;
}

// Types that benefit from an OS-native picker popup. WebView2 renders the
// input but doesn't always auto-open the picker on click, so we call
// showPicker() explicitly.
const PICKER_TYPES = new Set(["date", "datetime-local", "time", "month", "week", "color"]);

export function Input({ label, className, passwordToggle, type, onClick, ...props }: InputProps) {
  const [showPassword, setShowPassword] = useState(false);
  const isPassword = type === "password" && passwordToggle;
  const inputType = isPassword ? (showPassword ? "text" : "password") : type;

  const handleClick = (e: MouseEvent<HTMLInputElement>) => {
    if (type && PICKER_TYPES.has(type)) {
      const el = e.currentTarget as HTMLInputElement & { showPicker?: () => void };
      try {
        el.showPicker?.();
      } catch {
        // showPicker throws if the input is not connected to a document
        // or if the browser blocks it — silently ignore and let native
        // behavior take over.
      }
    }
    onClick?.(e);
  };

  return (
    <div className="space-y-1.5">
      {label && <label className="block text-xs font-medium text-zinc-500">{label}</label>}
      <div className="relative">
        <input
          type={inputType}
          onClick={handleClick}
          className={`w-full bg-[#141419] border border-[#1e1e26] rounded-lg px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-600 focus:outline-none focus:border-amber-500/50 focus:ring-1 focus:ring-amber-500/20 transition-colors ${isPassword ? "pr-10" : ""} ${className || ""}`}
          {...props}
        />
        {isPassword && (
          <button
            type="button"
            onClick={() => setShowPassword(!showPassword)}
            className="absolute right-2.5 top-1/2 -translate-y-1/2 text-zinc-500 hover:text-zinc-300 transition-colors"
          >
            {showPassword ? <EyeOff size={16} /> : <Eye size={16} />}
          </button>
        )}
      </div>
    </div>
  );
}
