import type { ReactNode } from 'react';
import CodeBlock from '@theme/CodeBlock';
import Schema from "@site/static/schemas/protocolSchema.json";
import JSONSchemaViewer from "@theme/JSONSchemaViewer";
import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

const references = {
    "https://stepflow.org/schemas/protocolSchema.json": require("@site/static/schemas/protocolSchema.json"),
    "https://stepflow.org/schemas/workflowSchema.json": require("@site/static/schemas/workflowSchema.json"),
    "https://json-schema.org/draft/2020-12/schema": require("@site/static/schemas/jsonSchema.json")
}

type Schema = {
    schema: keyof typeof references,
    path?: string,
};


const resolver = {
    resolve: (ref: string) => {
        return new Promise((resolve, reject) => {
            if (ref in references) {
                console.log("Resolved reference: ", ref);
                resolve(references[ref])
            } else {
                reject(new Error(`Unknown reference: ${ref}`));
            }
        })
    }
};


export default function SchemaDisplay({ schema, path }: Schema): ReactNode {
    const schemaSource = references[schema];
    let source = schemaSource;
    const resolverOptions = {
        resolvers: {
            https: resolver,
            http: resolver,
        }
    }
    if (path) {
        for (const p of path.split("/")) {
            source = source[p];
        }
        resolverOptions["jsonPointer"] = `#/${path}`;
    }
    return (
        <Tabs>
            <TabItem value="viewer" label="Schema Viewer" default>
                <JSONSchemaViewer schema={schemaSource} resolverOptions={resolverOptions} />
            </TabItem>
            <TabItem value="source" label="Schema Source">
                <CodeBlock language="json-schema">{JSON.stringify(source, null, 2)}</CodeBlock>
            </TabItem>
        </Tabs>
    );
}
