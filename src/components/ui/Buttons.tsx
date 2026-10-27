/**
 * 基础按钮组件：主按钮 / 幽灵按钮 / 图标按钮。
 */

import type { ButtonHTMLAttributes, ReactNode } from "react";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "ghost" | "danger";
  size?: "sm" | "md";
  children?: ReactNode;
}

export function Button({ variant = "ghost", size = "md", className = "", children, ...rest }: ButtonProps) {
  const base =
    "inline-flex items-center justify-center gap-1.5 rounded-lg font-medium transition-all duration-150 active:scale-[0.97] disabled:opacity-60 disabled:pointer-events-none focus-visible:outline-1 focus-visible:outline-[var(--accent)]";
  const sizes = size === "sm" ? "px-2.5 py-1 text-xs" : "px-3.5 py-1.5 text-[13px]";
  const variants: Record<string, string> = {
    primary:
      "bg-[var(--btn-primary-bg)] text-[var(--btn-primary-text)] shadow-sm hover:bg-[var(--btn-primary-hover)] hover:shadow-md active:bg-[var(--btn-primary-active)]",
    ghost: "hover:bg-[var(--surface-hover)] text-[var(--text-primary)]",
    danger: "text-[var(--urgent-text)] hover:bg-[rgba(248,113,113,0.12)]",
  };
  return (
    <button type="button" className={`${base} ${sizes} ${variants[variant]} ${className}`} {...rest}>
      {children}
    </button>
  );
}

interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  title: string;
  children: ReactNode;
}

export function IconButton({ title, className = "", children, ...rest }: IconButtonProps) {
  return (
    <button
      type="button"
      title={title}
      aria-label={title}
      className={`inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-lg text-[var(--text-secondary)] transition-all duration-150 hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)] active:scale-90 ${className}`}
      {...rest}
    >
      {children}
    </button>
  );
}
