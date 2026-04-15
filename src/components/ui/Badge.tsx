const variants = {
  default: "bg-zinc-800 text-zinc-300",
  amber: "bg-amber-500/15 text-amber-400 border border-amber-500/20",
  green: "bg-emerald-500/15 text-emerald-400 border border-emerald-500/20",
  red: "bg-rose-500/15 text-rose-400 border border-rose-500/20",
  blue: "bg-sky-500/15 text-sky-400 border border-sky-500/20",
} as const;

interface BadgeProps {
  children: React.ReactNode;
  variant?: keyof typeof variants;
  className?: string;
}

export function Badge({ children, variant = "default", className }: BadgeProps) {
  return (
    <span className={`inline-flex items-center px-2.5 py-0.5 rounded-md text-xs font-medium ${variants[variant]} ${className || ""}`}>
      {children}
    </span>
  );
}
