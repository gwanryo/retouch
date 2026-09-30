import '@testing-library/jest-dom/vitest'
import { cleanup } from '@testing-library/react'
import { afterEach } from 'vitest'

// Vitest runs without `globals`, so Testing Library cannot register its own
// afterEach hook — unmount between tests explicitly to keep the DOM isolated.
afterEach(() => {
  cleanup()
})
