// Copyright 2017 The Rustw Project Developers.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

import React from 'react';
import { combineReducers } from 'redux';
import { OrderedMap } from 'immutable';
import { SHOW_LOADING, SHOW_ERROR, SHOW_SEARCH, SHOW_FIND, SHOW_SOURCE, SHOW_SOURCE_DIR, SHOW_SUMMARY } from './actions';

export const Page = {
    START: 'START',
    SOURCE: 'SOURCE',
    SOURCE_DIR: 'SOURCE_DIR',
    SEARCH: 'SEARCH',
    FIND: 'FIND',
    SUMMARY: 'SUMMARY',
    LOADING: 'LOADING',
    INTERNAL_ERROR: 'INTERNAL_ERROR',
};

const initialState = {
    page: { type: Page.START }
};

export const rustwReducer = combineReducers({
    page
});

function page(state = { type: Page.START }, action) {
    switch (action.type) {
        case SHOW_LOADING:
            return { type: Page.LOADING };
        case SHOW_SEARCH:
            return {
                type: Page.SEARCH,
                defs: action.defs,
                refs: action.refs,
            };
        case SHOW_FIND:
            return {
                type: Page.FIND,
                results: action.results,
            };
        case SHOW_SOURCE:
            return {
                type: Page.SOURCE,
                path: action.path,
                lines: action.lines,
                lineStart: action.lineStart,
                highlight: action.highlight
            };
        case SHOW_SOURCE_DIR:
            return {
                type: Page.SOURCE_DIR,
                name: action.name,
                files: action.files,
            };
        case SHOW_SUMMARY:
            return {
                type: Page.SUMMARY,
                ...actions.data,
            };
        default:
            return state;
    }
}
