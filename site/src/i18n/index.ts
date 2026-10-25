import en from './en';
import es from './es';

export const dictionaries = { en, es };
export type Lang = keyof typeof dictionaries;
export type Dict = typeof en;

export const github = 'https://github.com/suitux/Backspn';
export const releases = `${github}/releases`;
