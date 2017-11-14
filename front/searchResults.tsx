// Copyright 2017 The Rustw Project Developers.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

import * as React from 'react';
import * as utils from './utils';

const API_URL = process.env.API;
declare const $: any;

interface ResultSetProps {
    input: Array<any>,
    kind: string,
    getSource?: (path :string, highlight :any) => any,
}
 
interface ResultSetState {}

class ResultSet extends React.Component<ResultSetProps, ResultSetState> {
    constructor(props: ResultSetProps) {
        super(props);
    }

    componentDidMount() {
        $(".src_link").removeClass("src_link");
        highlight_needle(this.props.input, this.props.kind);
    }

    render() {
        const { input, kind } = this.props;
        let count = 0;
        let result = input.map((r: any) => {
            let lines = r.lines.map((l: any) => {
                const lineId = `snippet_line_number_${kind}_${count}_${l.line_start}`;
                const snippetId = `snippet_line_${kind}_${count}_${l.line_start}`;
                const lineClick = (e: any) => {
                    const highlight = {
                        "line_start": l.line_start,
                        "line_end": l.line_start,
                        "column_start": 0,
                        "column_end": 0
                    };
                    this.props.getSource(r.file_name, highlight);
                    e.preventDefault();
                };
                const snippetClick = (e: any) => {
                    const highlight = {
                        "line_start": l.line_start,
                        "line_end": l.line_end,
                        "column_start": l.column_start,
                        "column_end": l.column_end
                    };
                    this.props.getSource(r.file_name, highlight);
                    e.preventDefault();
                };
                return (<div key={`${kind}-${l.line_start}`}>
                    <span className="div_span_src_number">
                        <div className="span_src_number" id={lineId} onClick={lineClick}>{l.line_start}</div>
                    </span>
                    <span className="div_span_src">
                        <div className="span_src" id={snippetId} onClick={snippetClick} dangerouslySetInnerHTML={{__html: l.line}} />
                    </span>
                    <br />
                </div>);
            });
            const onClick = (e: any) => {
                this.props.getSource(r.file_name, {});
                e.preventDefault();
            };
            count += 1;
            return (<div key={`${kind}-${r.file_name}`}>
                <div className="div_search_file_link" onClick={onClick}>{r.file_name}</div>
                <div className="div_all_span_src">
                    {lines}
                </div>
            </div>);
        })

        return (<div className="div_search_results">
            {result}
        </div>);
    }
}


interface SearchResultsProps {
  query: string
}

interface SearchResultsState {
    defs: Array<any>,
    refs: Array<any>
}

export class SearchResults extends React.Component<SearchResultsProps, SearchResultsState> {
    constructor(props: SearchResultsProps) {
        super(props);
        this.state = {
            defs: [],
            refs: []
        }
    }

    componentWillMount() {
        $.ajax({
            url: API_URL + 'search?needle=' + this.props.query,
            type: 'GET',
            dataType: 'JSON',
            cache: false
        })
        .done((json: any) => {                       
            this.setState({
              defs: json.defs,
              refs: json.refs
            });
        })
        .fail(function (xhr: any, status: any, errorThrown: any) {
            console.log("error: " + errorThrown + "; status: " + status);
        });
    }

    render() {
        if (!this.state.defs) {
            return noResults();
        } else {
            return (<section id="search_results" className="search-results">
                <div className="div_search_title">Definitions:</div>
                <ResultSet input={this.state.defs} kind="def"/>
                <div className="div_search_title">References:</div>
                <ResultSet input={this.state.refs} kind="ref"/>
            </section>);
        }
    }
}

function noResults() {
    return <span className="div_search_no_results">No results found</span>;
}

function highlight_needle(results: Array<any>, tag: string) {
    results.map((file: any, index: number) => {
        file.lines.map((line: any) => {
            line.line_end = line.line_start;
            utils.highlight_spans(line,
                                  null,
                                  `snippet_line_${tag}_${index}_`,
                                  "selected");
        })
    })
}