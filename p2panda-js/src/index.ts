export type Resolved<T> = T extends PromiseLike<infer U> ? Resolved<U> : T;

export { default as Session } from './session';
export { default as wasm } from './wasm';
