#!/usr/bin/env node
// Engine sample for JavaScript: a shebang and a header comment, then declarations.

/** Documented. */
export async function* stream(source) {}

export class Store {
  #items = [];
  get size() {
    return this.#items.length;
  }
  static create() {}
}

const handler = function (event) {},
  pattern = /[a-z]+/g;
let { a, b } = load();
module.exports = { handler };
export default () => {};
test.each([1, 2])('adds %i', (n) => {});
