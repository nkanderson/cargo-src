// Copyright 2017 The Rustw Project Developers.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

import * as React from 'react';

import * as utils from './utils';
import { DirView } from './dirView';
import { SourceView } from './srcView';

const API_URL = process.env.API;
declare const $: any;

interface Props {
  path: string,
}

// TODO: consider switching path to default prop with value of 'src'
// interface DefaultProps {
//   path: string,
// }

interface State {
  isFile: boolean,
  lines?: Array<string>,
  files?: any,
  highlight?: any,
  lineStart?: any,
  getSource?: (path :string) => any,
  name: any,
}

export class CodeBrowser extends React.Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = {
      isFile: false,
      lines: [],
      files: [],
      highlight: [],
      lineStart: [],
      getSource: (path) => {},
      name: this.props.path,
    }
  }

  componentWillMount() {
    const path = this.props.path;
    $.ajax({
        url: API_URL + 'src/' + path,
        type: 'GET',
        dataType: 'JSON',
        cache: false
    })
    .done((json: any) => {
        if (json.Directory) {
            this.setState({
              isFile: false,
              files: json.Directory.files,
              lines: [],
              highlight: [],
              lineStart: [],
              name: this.props.path,
            });
        } else if (json.Source) {
            this.setState({
              isFile: true,
              lines: json.Source.lines,
              highlight: [],
              lineStart: [],
            });
        } else {
            console.log("Unexpected source data.")
            console.log(json);
        }
    })
    .fail(function (xhr: any, status: any, errorThrown: any) {
        // console.log(errStr);
        console.log("error: " + errorThrown + "; status: " + status);

    });
  }

  render() {
    const path = this.props.path.split('/');    

    if (this.state.isFile) {
      return <SourceView path={path} lines={this.state.lines} highlight={this.state.highlight} scrollTo={this.state.lineStart} />
    } else {
      return <DirView file={this.state.name} files={this.state.files} getSource={this.state.getSource} />;
    }
  }
}
