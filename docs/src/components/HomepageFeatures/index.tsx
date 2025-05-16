import type { ReactNode } from 'react';
import clsx from 'clsx';
import Heading from '@theme/Heading';
import styles from './styles.module.css';


type FeatureItem = {
  title: string;
  image: any;
  description: ReactNode;
};

const FeatureList: FeatureItem[] = [
  {
    title: 'Workflow Description',
    image: require("@site/static/img/workflow_description.png").default,
    description: (
      <>
        StepFlow provides a workflow specification supporting conditional execution,
        looping, and dynamically generated workflows and components.
      </>
    ),
  },
  {
    title: 'Workflow Runtime',
    image: require("@site/static/img/workflow_execution.png").default,
    description: (
      <>
        StepFlow includes a performant workflow runtime supporting
        asynchronous execution, caching, and a plugin system for adding
        custom behavior.
      </>
    ),
  },
  {
    title: 'Workflow Protocol',
    image: require("@site/static/img/workflow_protocol.png").default,
    description: (
      <>
        StepFlow defines a JSON-RPC based protocol similar to MCP for
        providing components to the workflow.
      </>
    ),
  },
];

function Feature({ title, image, description }: FeatureItem) {
  return (
    <div className={clsx('col col--4')}>
      <div className="text--center">
        <img src={image} alt={title} className={styles.featureImage} />
      </div>
      <div className="text--center padding-horiz--md">
        <Heading as="h3">{title}</Heading>
        <p>{description}</p>
      </div>
    </div>
  );
}

export default function HomepageFeatures(): ReactNode {
  return (
    <section className={styles.features}>
      <div className="container">
        <div className="row">
          {FeatureList.map((props, idx) => (
            <Feature key={idx} {...props} />
          ))}
        </div>
      </div>
    </section>
  );
}
