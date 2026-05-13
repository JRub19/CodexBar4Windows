// Phase 3 D10: a single row inside the popup footer. 28 px tall, 12 px
// inner padding, 18 px icon column, 8 px icon-to-title gap. Title is
// 13 px, subtitle is 11 px secondary. Optional accelerator badge sits
// at the far right with 0.02em letter spacing.

interface Props {
  icon: React.ReactNode;
  title: string;
  subtitle?: string | null;
  accelerator?: string | null;
  destructive?: boolean;
  disabled?: boolean;
  /** Visual emphasis. "accent" uses the system accent color for the
   *  title text — used by the conditional update-ready row. */
  variant?: "default" | "accent";
  onClick: () => void;
}

export function ActionRow({
  icon,
  title,
  subtitle,
  accelerator,
  destructive = false,
  disabled = false,
  variant = "default",
  onClick,
}: Props) {
  const classes = ["action-row"];
  if (destructive) classes.push("action-row--destructive");
  if (variant === "accent") classes.push("action-row--accent");
  return (
    <button
      type="button"
      className={classes.join(" ")}
      onClick={onClick}
      disabled={disabled}
    >
      <span className="action-row__icon" aria-hidden="true">
        {icon}
      </span>
      <span className="action-row__text">
        <span className="action-row__title">{title}</span>
        {subtitle ? (
          <span className="action-row__subtitle">{subtitle}</span>
        ) : null}
      </span>
      {accelerator ? (
        <span className="action-row__accelerator">{accelerator}</span>
      ) : null}
    </button>
  );
}
