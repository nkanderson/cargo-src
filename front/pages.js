// Copyright 2017 The Rustw Project Developers.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

import * as React from 'react';
import { Route } from 'react-router-dom';
import qs from 'query-string';

import { SidebarController } from './Sidebar';
import { CodeBrowser } from './codeBrowser';
import { SearchResults } from "./searchResults";

export const PageTemplate = () => {
  return <div id="div_main">
      <SidebarController page="this.props.page" />
      <Route exact path="/" component={Home} />
      <Route path="/search" component={Search} />
      <Route path="/browse/*" component={Source} />
    </div>
}

export const Home = () =>
  <CodeBrowser path="browse/src" />

export const Search = ({ location }) => {
  const query = qs.parse(location.search).needle;
  return <SearchResults query={query} />
}

export const Source = ({match}) =>
  <CodeBrowser path={match.params[0]} />
  