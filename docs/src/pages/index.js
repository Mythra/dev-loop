import React from 'react';
import classnames from 'classnames';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import useBaseUrl from '@docusaurus/useBaseUrl';
import styles from './styles.module.css';

const features = [
  {
    title: <>Sustainable</>,
		alt: '',
    imageUrl: 'img/dl-renew.svg',
    description: (
      <>
        Dev-Loop was built from the ground up to help your codebase be sustainable. Whether
			  that&apos;s moving files, or reusing tasks easier.
      </>
    ),
  },
  {
    title: <>Extend</>,
		alt: '',
    imageUrl: 'img/dl-extendable.svg',
    description: (
      <>
        Dev-Loop helps you extend your tasks to run in things like docker containers
			  without changing your code at all.
      </>
    ),
  },
  {
    title: <>Build</>,
		alt: '',
    imageUrl: 'img/dl-build.svg',
    description: (
      <>
        Dev-Loop makes it easier than ever before to build complex multi-staged builds, and
			  parallel tasks.
      </>
    ),
  },
];

function Feature({imageUrl, title, description, alt}) {
  const imgUrl = useBaseUrl(imageUrl);
  return (
    <div className={classnames('col col--4', styles.feature)}>
      {imgUrl && (
        <div className="text--center">
          <img className={styles.featureImage} src={imgUrl} alt={alt} />
        </div>
      )}
      <h3>{title}</h3>
      <p>{description}</p>
    </div>
  );
}

function Home() {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  return (
    <Layout
      title={`${siteConfig.title} Docs`}
      description="A Localized Task Runner">
      <header className={classnames('hero hero--primary', styles.heroBanner)}>
        <div className="container">
          <h1 className="hero__title">{siteConfig.title}</h1>
          <p className="hero__subtitle">{siteConfig.tagline}</p>
          <div className={styles.buttons}>
            <Link
              className={classnames(
                'button button--outline button--secondary button--lg',
                styles.getStarted,
              )}
              to={useBaseUrl('docs/introduction/getting-started')}>
              Get Started
            </Link>
          </div>
        </div>
      </header>
      <main>
        {features && features.length && (
          <section className={styles.features}>
            <div className="container">
              <div className="row">
                {features.map((props, idx) => (
                  <Feature key={idx} {...props} />
                ))}
              </div>
            </div>
          </section>
        )}
      </main>
    </Layout>
  );
}

export default Home;
