import {
  RefreshCw,
  Settings,
  Info,
  Power,
  Download,
  Plus,
  Check,
  X,
  Pause,
  Play,
  AlertCircle,
  AlertTriangle,
  Sparkles,
  ChevronRight,
  ChevronLeft,
  Copy,
  ExternalLink,
  Search,
  Trash2,
  Cog,
  Layers,
  Bell,
  Palette,
  Keyboard,
  BarChart3,
  Sliders,
  LogOut,
  type LucideIcon,
  type LucideProps,
} from "lucide-react";

// Single source of truth for popup + settings iconography. Lucide
// icons are MIT-licensed, inline SVG, stroke-based — they render
// pixel-crisp at any zoom and never depend on font availability
// (which is what made Segoe Fluent icons unreliable on some Win10
// machines and on systems with missing fonts).

const ICONS: Record<string, LucideIcon> = {
  refresh: RefreshCw,
  settings: Settings,
  info: Info,
  power: Power,
  download: Download,
  plus: Plus,
  check: Check,
  close: X,
  pause: Pause,
  play: Play,
  error: AlertCircle,
  warning: AlertTriangle,
  sparkles: Sparkles,
  chevronRight: ChevronRight,
  chevronLeft: ChevronLeft,
  copy: Copy,
  externalLink: ExternalLink,
  search: Search,
  trash: Trash2,
  cog: Cog,
  layers: Layers,
  bell: Bell,
  palette: Palette,
  keyboard: Keyboard,
  chart: BarChart3,
  sliders: Sliders,
  logout: LogOut,
};

export type IconName = keyof typeof ICONS;

interface Props extends Omit<LucideProps, "ref"> {
  name: IconName;
}

export function Icon({ name, size = 16, strokeWidth = 1.5, ...rest }: Props) {
  const Component = ICONS[name];
  if (!Component) {
    // eslint-disable-next-line no-console
    console.warn(`[Icon] unknown name "${name}"`);
    return null;
  }
  return <Component size={size} strokeWidth={strokeWidth} {...rest} />;
}
