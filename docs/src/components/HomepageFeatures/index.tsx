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
    title: '⚙️ Build Locally, Scale Safely',
    Svg: require("@site/static/img/ProductionWorkflows.svg").default,
    description: (
      <>
        Develop and test workflows locally with confidence they’ll run reliably at enterprise scale without architectural changes.
      </>
    ),
  },
  {
    title: '🔐 Secure, Scalable Components',
    Svg: require("@site/static/img/SecureWorkflows.svg").default,
    description: (
      <>
        Configurable isolation for component execution provides security, resource controls, and production scalability.
      </>
    ),
  },
  {
    title: '🌐 Tool-Agnostic Workflows',
    Svg: require("@site/static/img/OpenWorkflows.svg").default,
    description: (
      <>
        Build and execute workflows using any editor, framework, or AI assistant without being locked into specific platforms.
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
