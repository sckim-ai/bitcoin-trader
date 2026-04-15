import { type ButtonHTMLAttributes } from "react";

const variants = {
  primary: "bg-amber-500 hover:bg-amber-400 text-black font-semibold shadow-lg shadow-amber-500/20",
  secondary: "bg-[#141419] hover:bg-[#1e1e26] text-zinc-300 border border-[#2a2a35]",
  ghost: "hover:bg-[#141419] text-zinc-400 hover:text-zinc-200",
  danger: "bg-rose-500/10 hover:bg-rose-500/20 text-rose-400 border border-rose-500/20",
  success: "bg-emerald-500/10 hover:bg-emerald-500/20 text-emerald-400 border border-emerald-500/20",
} as const;

const sizes = {
  sm: "px-3 py-1.5 text-xs rounded-lg",
  md: "px-4 py-2 text-sm rounded-lg",
  lg: "px-6 py-3 text-base rounded-xl",
} as const;

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: keyof typeof variants;
  size?: keyof typeof sizes;
}

export function Button({ variant = "primary", size = "md", className, disabled, children, ...props }: ButtonProps) {
  return (
    <button
      className={`inline-flex items-center justify-center gap-2 transition-all duration-200 ${variants[variant]} ${sizes[size]} ${disabled ? "opacity-50 cursor-not-allowed" : "cursor-pointer"} ${className || ""}`}
      disabled={disabled}
      {...props}
    >
      {children}
    </button>
  );
}
