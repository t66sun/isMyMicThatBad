import { writeFileSync } from 'node:fs';
import { builtins } from '../src/app/src/model';
if (!process.argv[2]) throw new Error('Provide an isolated test fixture output path');
writeFileSync(process.argv[2], JSON.stringify(builtins.slice(3)), 'utf8');
