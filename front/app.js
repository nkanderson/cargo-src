// Copyright 2017 The Rustw Project Developers.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

import React from 'react';
import ReactDOM from 'react-dom';
import { BrowserRouter as Router } from 'react-router-dom';
import { Provider, connect } from 'react-redux';
import { createStore, applyMiddleware } from 'redux';
import thunk from 'redux-thunk';
import { rustwReducer, Page } from './reducers';
import * as actions from './actions';
import styles from './rustw.css';

import * as utils from './utils';
import { PageTemplate } from './pages';
import { TopBarController } from './topbar';
import { FindResults, SearchResults } from "./search";
import { DirView } from './dirView';
import { SourceViewController } from './srcView';
import { Summary } from './summary';

// TODOs in build

class RustwApp extends React.Component {
    componentWillMount() {

        // history.replaceState(MAIN_PAGE_STATE, "");
        // window.onpopstate = onPopState;
    }

    componentDidMount() {
        $.ajax({
            dataType: "json",
            url: "/config",
            success: (data) => {
                CONFIG = data;
            },
            error: () => {
              CONFIG = {
                  "build_command": "cargo check",
                  "edit_command": "",
                  "unstable_features": false,
                  "port": 7878,
                  "demo_mode": false,
                  "demo_mode_root_path": "",
                  "context_lines": 2,
                  "build_on_load": false,
                  "source_directory": "src",
                  "save_analysis": true,
                  "vcs_link": ""
              };
            },
            async: false,
        });
        store.dispatch(actions.getSource(CONFIG.source_directory));
    }

    render() {
        let divMain;
        switch (this.props.page.type) {
            case Page.SEARCH:
                divMain = <div id="div_search_results"><SearchResults defs={this.props.page.defs} refs={this.props.page.refs} /></div>;
                break;
            case Page.FIND:
                divMain = <div id="div_search_results"><FindResults results={this.props.page.results} /></div>;
                break;
            case Page.SOURCE:
                divMain = <SourceViewController path={this.props.page.path} lines={this.props.page.lines} highlight={this.props.page.highlight} scrollTo={this.props.page.lineStart} />;
                break;
            case Page.SOURCE_DIR:
                divMain = <DirView file={this.props.page.name} files={this.props.page.files} getSource={this.props.getSource} />;
                break;
            case Page.LOADING:
                divMain = <div id="div_loading">Loading...</div>;
                break;
            case Page.SUMMARY:
                divMain = <Summary breadCrumbs={this.props.page.breadCrumbs} parent={this.props.page.parent} signature={this.props.page.signature} doc_summary={this.props.page.doc_summary} doc_rest={this.props.page.doc_rest} children={this.props.page.children} />;
                break;
            case Page.INTERNAL_ERROR:
                divMain = "Server error?";
                break;
            case Page.START:
            default:
                divMain = null;
        }
        return <div id="div_app">
            <TopBarController />
            <div id="div_main">
                {divMain}
            </div>
        </div>;
    }
}

const mapStateToProps = (state) => {
    return {
        page: state.page,
    }
}

const mapDispatchToProps = (dispatch) => {
    return {
        getSource: (fileName, lineStart) => dispatch(actions.getSource(fileName, lineStart)),
    }
}

const AppController = connect(
    mapStateToProps,
    mapDispatchToProps
)(RustwApp);

let store = createStore(rustwReducer, applyMiddleware(thunk));

export function renderApp() {
    ReactDOM.render(
        <Provider store={store}>
            <Router>
                <div id="div_app">
                    <PageTemplate />
                </div>
            </Router>
        </Provider>,
        document.getElementById('container')
    );
}
