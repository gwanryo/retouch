import { CaretLeft, CaretRight } from '@phosphor-icons/react'
import { useId, useState, type CSSProperties } from 'react'
import { Bezel } from './Bezel'

const SRC = `${import.meta.env.BASE_URL}demo/sunset.jpg`
const SRC_SMALL = `${import.meta.env.BASE_URL}demo/sunset-800.jpg`

/**
 * Before/after split on a real photograph. The left side is the flat
 * original, the right side the corrected answer (CSS filters stand in for
 * the engine until Plan 2). Dragging the handle is the game's core gesture.
 */
export function SplitDemo() {
  const [split, setSplit] = useState(52)
  const [touched, setTouched] = useState(false)
  const id = useId()

  return (
    <figure className="m-0 select-none">
      <Bezel>
        <div
          className={`relative aspect-[4/3] ${touched ? '' : 'split-auto'}`}
          style={{ '--split': `${split}%` } as CSSProperties}
        >
          <Photo alt="" className="[filter:saturate(.62)_contrast(.86)_brightness(.94)]" priority />
          <Photo
            alt="같은 사진의 정답 보정: 따뜻하고 대비가 살아 있는 석양 풀밭"
            className="[filter:saturate(1.22)_contrast(1.12)_brightness(1.03)] [clip-path:inset(0_0_0_var(--split))]"
          />

          {/* Handle */}
          <div aria-hidden className="pointer-events-none absolute inset-y-0 w-px bg-white/90 mix-blend-difference" style={{ left: 'var(--split)' }}>
            <div className="absolute top-1/2 left-1/2 flex size-11 -translate-x-1/2 -translate-y-1/2 items-center justify-center gap-0.5 rounded-full bg-black/35 text-white ring-1 ring-white/60 backdrop-blur-sm">
              <CaretLeft size={12} weight="regular" />
              <CaretRight size={12} weight="regular" />
            </div>
          </div>

          <input
            id={id}
            type="range"
            min={8}
            max={92}
            value={split}
            aria-label="원본과 정답 비교 위치"
            aria-valuetext={`정답 ${split}% 표시`}
            className="split-range absolute inset-0 z-10 opacity-0"
            onPointerDown={() => setTouched(true)}
            onChange={(e) => {
              setTouched(true)
              setSplit(Number(e.target.value))
            }}
          />
        </div>
      </Bezel>

      <figcaption className="mt-3 grid grid-cols-3 px-2 font-mono text-[11px] tracking-[0.08em] text-muted">
        <span>원본</span>
        <span className="text-center">드래그해서 비교</span>
        <span className="text-right text-accent">정답</span>
      </figcaption>
    </figure>
  )
}

function Photo({ className, alt, priority = false }: { className: string; alt: string; priority?: boolean }) {
  return (
    <img
      src={SRC}
      srcSet={`${SRC_SMALL} 800w, ${SRC} 1600w`}
      sizes="(min-width: 1024px) 640px, 100vw"
      width={1600}
      height={1200}
      alt={alt}
      loading={priority ? 'eager' : 'lazy'}
      fetchPriority={priority ? 'high' : 'auto'}
      decoding="async"
      draggable={false}
      className={`absolute inset-0 size-full object-cover ${className}`}
    />
  )
}
