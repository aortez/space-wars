use engine_agent::{
    brain_assembly::Definition,
    ship_arrival,
    training_history::{Protocol, Record},
    training_job::Recipe,
};

#[derive(Clone)]
pub(super) struct Link {
    pub brain: bool,
    pub id: String,
}
#[derive(Clone)]
pub(super) struct Sample {
    pub protocol: Protocol,
    pub label: String,
    pub value: String,
}
pub(super) struct Page {
    pub title: String,
    pub body: String,
    pub link: Option<Link>,
    pub sample: Option<Sample>,
}
fn page(title: impl Into<String>, body: impl Into<String>) -> Page {
    Page {
        title: title.into(),
        body: body.into(),
        link: None,
        sample: None,
    }
}
fn genome(id: &str) -> Option<Link> {
    Some(Link {
        brain: false,
        id: id.into(),
    })
}
fn protocol(domain: &str, runtime: &str, value: &impl serde::Serialize) -> Protocol {
    Protocol::new(domain, runtime, value).expect("validated historical protocol")
}
fn sample(
    domain: &str,
    runtime: &str,
    definition: &impl serde::Serialize,
    label: String,
    value: String,
) -> Option<Sample> {
    Some(Sample {
        protocol: protocol(domain, runtime, definition),
        label,
        value,
    })
}
pub(super) fn pages(record: &Record, validation: bool) -> Vec<Page> {
    match record {
        Record::Run(r) if validation => {
            let mut pages = vec![page(
                "Monitoring history",
                format!(
                    "{} saved validation result(s)\nOutside training selection; previously inspected cases.\nMissing or pending results have no score.\nOverlapping cases are not independent evidence.",
                    r.validation.len()
                ),
            )];
            for v in &r.validation {
                let cases: Vec<_> = v.cases.iter().map(|c| &c.case).collect();
                let definition = (&v.suite, &r.task.contract, &cases);
                let label = format!("{} · gen {}", r.id, v.generation);
                let value = format!("{} / {} arrivals", v.arrivals(), v.cases.len());
                pages.push(Page { title: format!("Gen {} · {}", v.generation, v.suite),
                    body: format!("Validation · {value}\nGenome: {}\n{} cases; browse each measurement with the detail arrows.", v.genome_id, v.cases.len()),
                    link: genome(&v.genome_id), sample: sample("validation_suite_v1", &r.runtime, &definition, label.clone(), value) });
                for c in &v.cases {
                    let value = format!(
                        "{:?} · {} ticks · distance {:.2} · speed {:.2}",
                        c.outcome, c.elapsed_ticks, c.distance, c.relative_speed
                    );
                    pages.push(Page {
                        title: format!("Gen {} · {}", v.generation, c.case.name),
                        body: format!(
                            "Validation · {value}\nCase seed {} · budget {} ticks\nGenome: {}",
                            c.case.seed, c.case.max_ticks, v.genome_id
                        ),
                        link: genome(&v.genome_id),
                        sample: sample(
                            "validation_case_v1",
                            &r.runtime,
                            &(&v.suite, &r.task.contract, &c.case),
                            label.clone(),
                            value,
                        ),
                    });
                }
            }
            pages
        }
        Record::Run(r) => {
            let case_count = r
                .task
                .configuration
                .get("cases")
                .and_then(|c| c.as_array())
                .map_or(0, Vec::len);
            let state = if r.complete {
                "Training generations complete"
            } else {
                "Incomplete checkpoint · pause / stop / interruption not recorded"
            };
            let mut pages = vec![
                page(
                    "Overview",
                    format!(
                        "{state}\n{} / {} generations · {} evaluated candidates\n{} recorded cases per candidate · {:.1}s evaluation\nSeed {} · population {} · {:?}\n{}",
                        r.winners.len(),
                        r.config.generations,
                        r.evaluated_candidates,
                        case_count,
                        r.evaluation_millis as f64 / 1000.0,
                        r.config.seed,
                        r.config.population,
                        r.recipe,
                        r.compatibility.as_deref().unwrap_or(
                            "Current runtime and recipe; history never resumes or replaces a job."
                        )
                    ),
                ),
                page(
                    "Build identity",
                    format!(
                        "Run: {}\nRuntime: {}\nSource revision was not recorded in this save.",
                        r.id, r.runtime
                    ),
                ),
                page(
                    "Evolution configuration",
                    format!(
                        "Elites {} · tournament {}\nMutation probability {} · scale {}\nInitial scale {} · parameter limit {}\nInitial genomes: {} ({})",
                        r.config.elites,
                        r.config.tournament_size,
                        r.config.mutation_probability,
                        r.config.mutation_scale,
                        r.config.initial_scale,
                        r.config.parameter_limit,
                        r.initial_genomes.len(),
                        if r.selected_initial_genomes {
                            "selected by user"
                        } else {
                            "recipe defaults"
                        }
                    ),
                ),
                page(
                    "Training protocol",
                    format!(
                        "{}\nProtocol: {}\nFitness terms: {}",
                        r.task.id,
                        protocol("training_fitness_v1", &r.runtime, &(&r.recipe, &r.task))
                            .definition,
                        match r.recipe {
                            Recipe::StationaryArrival =>
                                "arrivals, position, velocity, spare ticks",
                            Recipe::MixedArrival =>
                                "stationary arrivals, total arrivals, position, velocity, spare ticks",
                            Recipe::DiagnosticLine => "arrivals, proximity, spare ticks",
                        }
                    ),
                ),
            ];
            for (i, id) in r.initial_genomes.iter().enumerate() {
                let mut p = page(
                    format!("Initial genome {}", i + 1),
                    format!(
                        "Exact starting genome\n{id}\nOpen looks up the saved artifact; missing exports remain missing."
                    ),
                );
                p.link = genome(id);
                pages.push(p);
            }
            for (w, p) in r.winners.iter().zip(&r.progress) {
                let value = format!(
                    "{} / {case_count} arrivals · fitness {:?}",
                    w.arrivals, w.fitness
                );
                pages.push(Page {
                    title: format!("Generation {} winner", w.generation),
                    body: format!(
                        "Training · {value}\nPopulation median: {} arrivals\nGenome: {}",
                        p.median_arrivals, w.genome_id
                    ),
                    link: genome(&w.genome_id),
                    sample: sample(
                        "training_fitness_v1",
                        &r.runtime,
                        &(&r.recipe, &r.task),
                        format!("{} · gen {}", r.id, w.generation),
                        value,
                    ),
                });
            }
            pages
        }
        Record::Trial(t) => {
            let link = t
                .assembly
                .as_ref()
                .map(|a| Link {
                    brain: true,
                    id: a.id.clone(),
                })
                .or_else(|| genome(&t.brain.controller));
            let value = format!(
                "{:?} · {} ticks · distance {:.2} · speed {:.2}",
                t.outcome, t.elapsed_ticks, t.measure.distance, t.measure.relative_speed
            );
            // Candidate/controller identity is deliberately excluded; all host
            // behavior, cases, scoring version and runtime remain pinned.
            let mut host = t.brain.clone();
            host.controller.clear();
            // Version-one component receipts used this implicit arrival host.
            // Normalize it to the saved assembly's execution metadata, without
            // guessing a host for receipts from an unrecognized recipe.
            let implicit = (t.version == 1 && t.recipe == ship_arrival::RECIPE)
                .then(|| Definition::arrival(&t.brain.controller));
            let execution = t
                .assembly
                .as_ref()
                .map(|a| &a.definition)
                .or(implicit.as_ref());
            vec![
                Page {
                    title: t.case.name.clone(),
                    body: format!(
                        "Practice · {value}\nSeed {} · budget {} ticks\nGenome: {}\n{}",
                        t.case.seed,
                        t.case.max_ticks,
                        t.brain.controller,
                        if t.assembly.is_some() {
                            "Saved assembly receipt"
                        } else {
                            "Component practice receipt"
                        }
                    ),
                    link: link.clone(),
                    sample: sample(
                        "practice_case_v1",
                        &t.runtime,
                        &(
                            t.version,
                            &t.recipe,
                            &t.case,
                            &host,
                            execution.map(|d| {
                                (d.version, &d.host, &d.organism, &d.required_capabilities)
                            }),
                        ),
                        t.brain.controller.clone(),
                        value,
                    ),
                },
                Page {
                    title: "Brain identity".into(),
                    body: format!(
                        "Practice brain: {}\nAssembly: {}\nNo training generation is attributed to a practice receipt.",
                        t.brain_id,
                        t.assembly.as_ref().map_or("none", |a| a.id.as_str())
                    ),
                    link,
                    sample: None,
                },
                page(
                    "Build and scoring",
                    format!(
                        "{}\nRuntime: {}\nFitness: {:?}",
                        t.recipe, t.runtime, t.fitness
                    ),
                ),
            ]
        }
    }
}

#[cfg(test)]
mod tests;
