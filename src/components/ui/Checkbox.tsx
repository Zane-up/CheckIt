/**
 * 任务完成勾选框：带勾选动画的自定义复选框。
 */

interface CheckboxProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  size?: number;
  title?: string;
}

export function Checkbox({ checked, onChange, size = 18, title }: CheckboxProps) {
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      title={title}
      onClick={(e) => {
        e.stopPropagation();
        onChange(!checked);
      }}
      className={`flex shrink-0 items-center justify-center rounded-[6px] border transition-all duration-150 active:scale-90 ${
        checked
          ? "border-transparent bg-[var(--ok)]"
          : "border-[rgba(140,146,170,0.5)] hover:border-[var(--accent)]"
      }`}
      style={{ width: size, height: size }}
    >
      {checked && (
        <svg
          width={size - 4}
          height={size - 4}
          viewBox="0 0 16 16"
          fill="none"
          style={{ color: "rgba(10,30,20,0.9)" }}
        >
          <path
            className="checkmark-path"
            d="M3 8.5 L6.5 12 L13 4.5"
            stroke="currentColor"
            strokeWidth="2.2"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      )}
    </button>
  );
}
