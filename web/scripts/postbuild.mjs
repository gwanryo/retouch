// GitHub Pages has no server-side rewrite. Copying index.html to 404.html makes
// deep links such as /retouch/c/012 load the SPA instead of GitHub's 404 page
// (spec AC-H4a). `.nojekyll` stops Pages from ignoring files that start with "_".
import { copyFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'

const dist = resolve(import.meta.dirname, '..', 'dist')
copyFileSync(resolve(dist, 'index.html'), resolve(dist, '404.html'))
writeFileSync(resolve(dist, '.nojekyll'), '')
console.log('postbuild: wrote dist/404.html and dist/.nojekyll')
