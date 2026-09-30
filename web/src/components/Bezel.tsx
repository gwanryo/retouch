import type { ReactNode } from 'react'

/**
 * Double-bezel container: an outer tray with a hairline ring and an inner
 * core with a concentric (smaller) radius, so cards read as machined
 * hardware instead of a flat rectangle on the page.
 */
export function Bezel({ children, className = '', inner = '' }: { children: ReactNode; className?: string; inner?: string }) {
  return (
    <div className={`rounded-[2rem] bg-fg/[0.04] p-1.5 ring-1 ring-fg/[0.06] ${className}`}>
      <div className={`overflow-hidden rounded-[calc(2rem-0.375rem)] bg-card shadow-[var(--bezel-highlight)] ${inner}`}>{children}</div>
    </div>
  )
}
