import type { ReactNode } from 'react';
import clsx from 'clsx';
import Heading from '@theme/Heading';
import styles from './styles.module.css';


type FeatureItem = {
  title: string;
  Svg: React.ComponentType<React.ComponentProps<'svg'>>;
  description: ReactNode;
};

const FeatureList: FeatureItem[] = [
  {
    title: '⚙️ Reliable, Scalable Execution',
    Svg: require("@site/static/img/ProductionWorkflows.svg").default,
    description: (
      <>
        Run workflows locally with confidence they’ll scale.
        Stepflow provides built-in durability and fault tolerance—ready for seamless transition to production-scale deployments.
      </>
    ),
  },
  {
    title: '🔐 Secure, Isolated Components',
    Svg: require("@site/static/img/SecureWorkflows.svg").default,
    description: (
      <>
        Each workflow step runs in a sandboxed process or container with strict resource and environment controls.
        Stepflow's design prioritizes security, reproducibility, and platform independence.
      </>
    ),
  },
  {
    title: '🌐 Open, Portable Standard',
    Svg: require("@site/static/img/OpenWorkflows.svg").default,
    description: (
      <>
        Build once, run anywhere.
        The Stepflow protocol defines an open workflow format that any framework or editor can use—enabling true portability and ecosystem integration.
      </>
    ),
  },
];

function Feature({ title, Svg, description }: FeatureItem) {
  return (
    <div className={clsx('col col--4')}>
      <div className="text--center">
        <Svg className={styles.featureImage} role="img" />
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
