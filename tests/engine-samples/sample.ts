// Engine sample for TypeScript: a header comment, then declarations of most kinds.

'use strict';

import fs = require('fs');

/** Documented. */
export async function run<T>(task: T, retries = 3): Promise<void> {}

@sealed
export abstract class Runner<T> extends Base implements Task {
  @log
  start(): void {}
  stop = () => {};
  private count = 0;
}

export const first = 1,
  second = (value: number): number => value;
interface Shape {
  size: number;
}
type Id = string | number;
const { width, height = measure() }: Size = box;
enum Role {
  Admin,
}
namespace Tools {
  export const x = 1;
}
declare module 'module' {}
declare global {
  interface Window {}
}
export function overloaded(a: string): void;
export { run as go };
export default function () {}
describe('runner', () => {
  it('starts', () => {});
});
