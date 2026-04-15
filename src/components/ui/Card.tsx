interface CardProps {
  children: React.ReactNode;
  className?: string;
  glow?: boolean;
}

export function Card({ children, className, glow }: CardProps) {
  return (
    <div
      className={`bg-[#0c0c0f] border border-[#1e1e26] rounded-xl ${glow ? "glow-border" : ""} ${className || ""}`}
      style={{ boxShadow: "inset 0 1px 0 rgba(255,255,255,0.03)" }}
    >
      {children}
    </div>
  );
}

export function CardHeader({ children, className }: { children: React.ReactNode; className?: string }) {
  return <div className={`px-5 py-4 border-b border-[#1e1e26] ${className || ""}`}>{children}</div>;
}

export function CardContent({ children, className }: { children: React.ReactNode; className?: string }) {
  return <div className={`px-5 py-4 ${className || ""}`}>{children}</div>;
}
