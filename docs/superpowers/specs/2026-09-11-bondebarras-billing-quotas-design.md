# bondebarras — quotas de la formule, stockage Actions, budget et rétention

- **Date :** 2026-09-11
- **Statut :** Design (arbitrages du 2026-09-11 intégrés, en attente de relecture utilisateur)
- **Auteur :** k@levilainpetit.dev (avec Claude Code)
- **Issues :** #12, #11, #13, #14, #15 de `systm-d/bondebarras`, dans cet ordre
- **Prérequis :** `feat/tui-3-colonnes` terminée (tâches 1 à 8 et revue finale). Ce travail
  se fait sur `feat/billing-quotas`, **empilée** sur elle.

## 1. Le défaut constaté

Le 2026-09-10, exec-d reçoit de GitHub « You have used 90% of the Actions storage
included ». L'audit qui suit montre que l'onglet Billing ne pouvait rien en dire, et qu'il
disait faux sur ce qu'il montrait :

| Ce que l'onglet montre | Ce qui est vrai | Issue |
|---|---|---|
| des montants en `€` | le rapport GitHub est en dollars US | #12 |
| exec-d (Team) à **50 %** de ses minutes | **33 %** : 1 004 sur 3 000, pas sur 2 000 | #11 |
| SecondBrain-io (Enterprise) à **901 %** | **36 %** : 18 016 sur 50 000 | #11 |
| rien sur le stockage Actions | 371,85 GB-heures, dont 359,88 pour `disconnected` | #13 |
| rien sur ce qui arrive au-delà du quota | budget 0 $ bloquant sur exec-d : l'usage s'arrête | #14 |
| rien sur la rétention | 90 jours, le réglage qui a fait déborder exec-d | #15 |

Une jauge fausse dans le sens du dépassement est pire qu'une absence de jauge : on apprend
à l'ignorer. Toute la conception qui suit tient en une règle déjà appliquée à la taille des
versions de paquets : **ne jamais afficher un chiffre que l'API ne donne pas.**

## 2. Décisions

Arbitrages du 2026-09-11. Chacun porte le coût s'il se révèle faux.

1. **Ordre et branche** : #12, puis #11 (les autres dépendent de la formule), #13, #14, #15 ;
   un seul plan, une seule branche `feat/billing-quotas` empilée sur `feat/tui-3-colonnes`.
   — Coût si faux : un rebase de branche.
2. **#12** : montants formatés `{:.2} $` (suffixe, même disposition que l'ancien `€`),
   jamais convertis. — Coût : une chaîne de format.
3. **#11, source** : `GET /orgs/{org}` → `plan.name`, lu dans `scan::overview` (étage 1) ;
   `None` si absent ou refusé, sans jamais faire tomber l'organisation. — Coût : un appel
   de plus par organisation à l'étage 1, déjà dégradable.
4. **#11, quota** : fonction pure `billing::included_minutes_for(plan: Option<&str>) ->
   Option<u64>` (`free` 2 000, `team` 3 000, `enterprise` 50 000 ; inconnue → `None`), qui
   **remplace** `FREE_MINUTES_PER_MONTH` comme source unique. — Coût : une ligne du `match`
   quand GitHub change un chiffre.
5. **#11, pas de quota, pas de pourcentage, nulle part** : la jauge montre le total et dit
   « formule inconnue ». — Coût : aucun ; l'inverse est le défaut corrigé.
6. **#11, en-tête** : « quota de la formule actuelle » ; en `enterprise`, une ligne dit que
   le quota est celui du compte entreprise, partagé, donc que la jauge est un minimum. —
   Coût : deux lignes de texte si la doc dit un jour autre chose.
7. **#11, colonne 3** : la jauge de minutes par dépôt de `feat/tui-3-colonnes`
   (`gauges::minutes_gauge_line`) passe au même quota par organisation (`None` → pas de
   `%`). — Coût : un paramètre.
8. **#11, JSON** : `plan` et `minutes_allowance` par organisation. — Coût : deux champs.
9. **#13, quota** : GB-heures incluses = Go inclus (`free` 0,5 ; `team` 2 ; `enterprise`
   50 ; inconnue → `None`) × heures **du mois affiché** (jours × 24, la formule de la doc).
   La ligne écrit sa base (« base 720 h »). L'observation des 744 h reste une mesure ouverte
   (§9), pas un choix silencieux. — Coût : la base à changer en un seul endroit
   (`billing::hours_in_month`), et un test qui dit pourquoi.
10. **#13, dépôts publics** : leur stockage est compté, et l'onglet le dit. — Coût : une
    jauge trop haute pour une organisation très publique, annoncée comme telle.
11. **#13, où** : la jauge de stockage vit dans l'**onglet Billing** (niveau organisation).
    Pour l'onglet Billing seulement, ceci **remplace** la phrase du §5 de la spec
    `feat/tui-3-colonnes` : « le stockage Actions est un débit en gigaoctet-heures, pas un
    niveau comparable à un plafond ». Cette phrase reste vraie pour la colonne 3, qui garde
    ses **deux** jauges par dépôt. — Coût : aucun sur la colonne 3.
12. **#13, colonne 2** : les GB-heures du mois courant apparaissent dans la colonne des
    dépôts (`tui/views/repos.rs`). — Coût : voir décision 18.
13. **#13, ⚠ des caches** : le ⚠ d'un dépôt au-delà du plafond réutilise
    `gauges::CACHE_CEILING_BYTES` (10 Gio), sans second littéral. — Coût : aucun.
14. **#13, ligne fixe** : supprimer des artefacts arrête l'accumulation mais ne rend pas les
    heures déjà comptées — **obligatoire**. — Coût : aucun.
15. **#13, JSON** : `storage_gbh`, `storage_allowance_gbh` par organisation, `storage_gbh`
    par dépôt. — Coût : trois champs.
16. **#14** : `GET /organizations/{org}/settings/billing/budgets`, paginé ; dégradation sur
    **tout** statut non-2xx ; « pas de budget » (`null`) et « budgets illisibles »
    (`budgets_readable: false`) jamais confondus ; un budget `SkuPricing` sur un SKU
    Actions est **signalé comme tel**, ni ignoré ni interprété ; lecture seule,
    définitivement ; le README documente les droits requis (admin ou gestionnaire de
    facturation) et ce que l'onglet montre sans eux ; au moins 90 % d'une jauge avec un
    budget bloquant → ligne d'avertissement. — Coût si le 90 % est mal placé : une
    constante.
17. **#15** : **lecture seule** dans ce tour : pas de `PUT`, pas de palier. Refus → `None`
    (« rétention illisible ») ; le README documente `admin:org` comme **optionnel**, requis
    seulement pour afficher la rétention ; mise en avant de 90 jours quand le stockage de
    l'organisation n'est pas négligeable (seuil au §7.3) ; les deux précisions de l'issue
    figurent dans l'onglet ; JSON `artifact_retention_days`. — Coût si faux : une tâche de
    suivi ajoute l'écriture, au palier 2.

Décisions prises par ce design, à relire :

18. **Colonne 2, forme** : les GB-heures d'un dépôt s'affichent sur une **ligne de détail**
    sous la ligne du dépôt (`↳ 359.9 GB-h ce mois`), et seulement pour un dépôt qui en a ce
    mois-ci ; le ⚠ se place devant la taille des caches, sur la ligne principale. La colonne 2
    fait `Length(38)` (`orgs::PANE_WIDTH`, amendement 2 de la spec `feat/tui-3-colonnes`,
    `6dd71df`), soit 36 cellules intérieures. La ligne de dépôt de la v0.5 en prend 35 (case 4,
    nom 10, âge ou classe 13, taille 8) ; la cellule du ⚠ (espace ou ⚠) prend la 36e, la
    dernière libre. Aucun chiffre de GB-heures ne tient donc sur la même ligne, et le nom ne
    rétrécit pas pour lui faire de la place. — Coût si faux : élargir la colonne 2 pour
    remonter le chiffre sur la ligne principale, ce qui déplace les seuils 100 / 78 que la spec
    `feat/tui-3-colonnes` dérive de `22 + 38 + 40` et `38 + 40`.
19. **« Mois courant »** (colonne 2 et JSON) : le mois calendaire UTC au moment du rendu ou
    du scan (`billing::month_of(Utc::now())`), pas le dernier mois présent dans le rapport.
    Le premier jour d'un mois sans usage lit donc 0, ce qui est vrai. — Coût : une ligne.
    Le JSON l'expose en `billing_month`.
20. **Budgets mal formés** : une entrée à laquelle manque l'un des cinq champs lus rend
    **toute** la liste illisible. Ignorer l'entrée pourrait faire dire « aucun budget :
    dépassement facturé » sur une organisation bloquée. — Coût : un `?`.
21. **Pagination des budgets** : au plus 10 pages de 100 ; si la dixième annonce encore une
    suite, la liste est illisible plutôt que partielle, pour la même raison. — Coût : une
    constante.
22. **Rétention mise en avant à 90 jours *ou plus*** : `maximum_allowed_days` va jusqu'à
    400, et 400 jours accumulent plus que 90. — Coût : un `>=` à changer en `==`.
23. **Montant d'un budget** : entier (`budget_amount`, typé `integer` par la doc), affiché
    avec le même formateur que les coûts (`0.00 $`). — Coût : aucun.
24. **Lignes de l'onglet d'au plus 56 caractères** pour tout texte qu'un test vérifie, afin
    qu'il reste visible dans le cadre à 60 colonnes. Les phrases longues passent sur deux
    lignes. — Coût : de la mise en page.

## 3. #12 — dollars

`tui/views/billing.rs` formate les coûts avec `€` alors que `pricePerUnit` vaut `0.006`
pour `Actions Linux`, le tarif en dollars de la doc. Correction : un formateur unique
`usd(amount) -> "{amount:.2} $"`, utilisé par la ligne de coûts et, plus tard, par les
budgets.

```
Coûts   brut 6.15 $   couvert 6.15 $   facturé 0.00 $
```

Test : rendu de l'onglet à toutes les largeurs de 60 à 200 — `6.15 $` présent, `€` absent.

## 4. #11 — le quota de la formule

### 4.1 Source et dégradation

`GET /orgs/{org}` → `plan.name` (`free`, `team`, `enterprise` observés). GitHub ne renvoie
`plan` qu'aux propriétaires. `api::orgs::plan(client, org) -> Option<String>` rend `None`
sur tout échec, et sur un corps sans `plan.name`. `OrgSummary.plan: Option<String>`.

L'appel rejoint les lectures dégradables de `scan::overview`, jointes entre elles
(`futures::join!`) : aucune ne dépend d'une autre, aucune ne fait tomber l'organisation.

### 4.2 Calcul

- `billing::included_minutes_for(plan)` — décision 4.
- **Pas de second calcul de pourcentage.** `tui::views::gauges::percent(used, ceiling) ->
  u64` (`pub(crate)` depuis `e635f36` sur `feat/tui-3-colonnes` — arrondi, jamais plafonné,
  `0` pour un plafond nul) est déjà partagé par les jauges de la colonne 3 et par
  `views::billing::gauge_line`. Ce design le réutilise tel quel : pour les minutes, pour le
  stockage (GB-heures passées en centièmes, la précision du rapport) et pour le seuil
  d'alerte des budgets. Aucun module pur n'en a besoin — le JSON ne publie aucun
  pourcentage, et `billing::nears_blocking_budget` reçoit celui que la jauge affiche. Un
  seuil calculé ailleurs pourrait dire 89 % quand la jauge affiche 90 %.

### 4.3 Onglet Billing

```
 exec-d · formule team
 2026-09 · quota de la formule actuelle

 Minutes équivalent-inclus
 1 004 / 3 000   ███  33 %
```

Formule inconnue :

```
 le-vilain-petit-dev · formule inconnue
 …
 1 004 min   formule inconnue, pas de quota
```

Enterprise, deux lignes sous l'en-tête :

```
 Formule enterprise : quota partagé par tout le compte
   entreprise, ces pourcentages sont des minimums.
```

### 4.4 Colonne 3

`gauges::minutes_gauge_line(used, is_public, allowance: Option<u64>, width)`. Un dépôt
public garde sa raison (0 %, gratuit) ; un dépôt privé sans quota connu lit
`Minutes  1004 min · formule inconnue, pas de quota`, sans `%`.

### 4.5 JSON

`plan` (chaîne ou `null`), `minutes_allowance` (entier ou `null`).

## 5. #13 — stockage Actions

### 5.1 Aucune requête de plus

Le `BillingReport` chargé à l'étage 1 porte déjà les lignes `sku = "Actions storage"`,
`unitType = "GigabyteHours"`, par dépôt et par mois. `billing.rs` les écartait volontairement
(`storage_line_items_do_not_pollute_the_minutes_gauge`) ; elles deviennent un axe à part :

- `storage_gbh(month) -> f64` : somme des seules lignes `Actions storage` en `GigabyteHours`.
- `storage_gbh_for_repo(month, repo) -> f64`.
- `storage_lines(month) -> Vec<StorageLine { repo, gbh }>`, agrégées par dépôt, du plus
  lourd au plus léger — même raison d'être que `minute_lines` : nommer le dépôt à corriger.
- `included_storage_gb_for(plan) -> Option<f64>`, `hours_in_month("YYYY-MM") -> Option<u32>`,
  `storage_quota(plan, month) -> Option<StorageQuota { gbh, hours }>`.

### 5.2 Onglet Billing

```
 Stockage Actions · GB-heures, dépôts publics compris
 371.85 / 1 440 GB-h   ██  26 %   base 720 h
    disconnected           359.88 GB-h
    ptitjardinier-app       11.21 GB-h
 Supprimer des artefacts arrête l'accumulation,
   mais ne rend pas les GB-heures déjà comptées.
```

Même limite de lignes que les minutes (`MAX_BREAKDOWN_LINES`, 8), même ligne « … et N
autre(s) dépôt(s) ». La jauge n'est pas plafonnée. Sans formule connue :
`371.85 GB-h   formule inconnue, pas de quota`.

Doc GitHub (vérifiée le 2026-09-11) : « Deleting artifacts reduces your current storage and
prevents future charges, but does not remove charges already recorded ». La doc est muette
sur la gratuité du stockage des dépôts publics : ils sont comptés (décision 10).

### 5.3 Colonne 2

- Ligne de détail `↳ 359.9 GB-h ce mois` sous un dépôt qui a des GB-heures ce mois-ci
  (décision 18) ; rien si la facturation est illisible.
- `⚠` devant la taille des caches quand `cache_bytes > CACHE_CEILING_BYTES` —
  strictement : 10 Gio tout rond, non ; 10 Gio + 1 octet, oui.

Ce que le ⚠ signifie, d'après la doc : les 10 Go inclus par dépôt sont dépassés ; GitHub
**évince** (ce que dit déjà la jauge de la colonne 3), ou **facture l'excédent au pic horaire
si la limite de cache du dépôt a été relevée**. Le README le dit ; la ligne n'a pas la place.

### 5.4 JSON

`billing_month` (`YYYY-MM`, décision 19), `storage_gbh` et `storage_allowance_gbh` (flottant
ou `null`) par organisation, `storage_gbh` (flottant ou `null`) par dépôt.

## 6. #14 — budget Actions

### 6.1 Source

`GET /organizations/{org}/settings/billing/budgets?per_page=100&page=N`, pages suivies tant
que `has_next_page` vaut `true` (absent → dernière page). Doc : réservé aux admins et aux
gestionnaires de facturation ; statuts annoncés 403, 404, 500. **Observé : 400
`{"message":"Unable to get budgets."}`** sur les trois organisations dont le compte n'est pas
propriétaire. `api::budgets::fetch` rend `None` sur tout échec (décisions 16, 20, 21).

`billing::Budget { budget_type, sku, scope, amount: u64, blocking: bool }`, lus depuis
`budget_type`, `budget_product_sku`, `budget_scope`, `budget_amount`,
`prevent_further_usage`. `OrgSummary.budgets: Option<Vec<Budget>>` : `None` = illisible,
`Some(vec![])` = lisible, aucun budget.

### 6.2 Sélection

- `actions_budget(&[Budget]) -> Option<&Budget>` : portée `organization`, type
  `ProductPricing`, SKU `actions`. Dans la réponse réelle d'exec-d (quatre budgets :
  codespaces, packages, actions, git_lfs), seul le troisième.
- `actions_sku_budgets(&[Budget]) -> Vec<&Budget>` : portée `organization`, type
  `SkuPricing`, SKU commençant par `actions` (insensible à la casse). Jamais observé ;
  signalé ligne par ligne, non pris en compte dans les avertissements.
- `nears_blocking_budget(percent, Option<&Budget>) -> bool` : `percent >= 90` et budget
  bloquant.

### 6.3 Onglet Billing

Sous l'en-tête, pour tous les cas :

| Cas | Ligne(s) |
|---|---|
| 0 $, bloquant | `Budget Actions : 0.00 $ · bloquant` |
| 5 $, bloquant | `Budget Actions : 5.00 $ · bloquant` |
| 5 $, non bloquant | `Budget Actions : 5.00 $ · alerte seule, sans blocage` |
| aucun | `Budget Actions : aucun, dépassement facturé sans plafond` / `  (si un moyen de paiement est enregistré)` |
| illisible | `Budget Actions : illisible` / `  (réservé aux admins et gestionnaires de facturation)` |
| SKU Actions | `Budget SKU actions_linux : 5.00 $ · bloquant` / `  signalé, non pris en compte par les avertissements` |

Sous une jauge à 90 % ou plus avec un budget Actions bloquant :

```
 ⚠ 95 % du quota de minutes, budget 0.00 $ bloquant :
   GitHub bloquera l'usage Actions au quota atteint.
```

```
 ⚠ 95 % du quota de minutes, budget 5.00 $ bloquant :
   facturé jusqu'à 5.00 $, puis usage Actions bloqué.
```

Même forme sous la jauge de stockage (`du quota de stockage`). Pas de jauge en pourcentage
(formule inconnue) : pas d'avertissement.

### 6.4 JSON

`budgets_readable` (booléen), `actions_budget` (`{"amount": 0, "blocking": true}` ou
`null`), `actions_sku_budgets` (tableau, `null` si illisible).

## 7. #15 — rétention des artefacts et journaux

### 7.1 Source

`GET /orgs/{org}/actions/permissions/artifact-and-log-retention` → `{"days": 90,
"maximum_allowed_days": 400}`. Doc : scope classique `admin:org`, ou permission fine
« Actions policies » ; statuts 200, 403, 404. `api::retention::fetch` rend `None` sur tout
échec et sur un corps sans `days` entier. `model::ArtifactRetention { days: u32,
maximum_allowed_days: Option<u32> }`, `OrgSummary.retention`.

**Lecture seule.** Le `PUT` existe (204) ; il n'est pas appelé dans ce tour (décision 17).

### 7.2 Onglet Billing

Sous le bloc stockage, « à côté du stockage » :

```
 Rétention artefacts et journaux : 90 j (max. 400 j)
```

Mise en avant (style d'avertissement) :

```
 ⚠ Rétention artefacts et journaux : 90 j (max. 400 j)
   c'est ce réglage qui fait durer le stockage
```

Illisible : `Rétention artefacts et journaux : illisible` / `  (scope admin:org requis pour
la lire)`.

En pied d'onglet, toujours :

```
 Note : retention-days, dans un workflow, fixe la durée
   de cet artefact, dans la limite de ce réglage.
 Note : un changement de rétention ne vaut que pour
   les nouveaux artefacts et journaux.
```

Vérifié le 2026-09-10 : après le passage d'exec-d à 7 jours, un APK de la veille gardait
son échéance au 2026-12-08.

### 7.3 Seuil de mise en avant

`billing::retention_worth_flagging(days, storage_gbh: Option<f64>)` : `days >= 90` **et**
GB-heures du mois affiché `>= 36`.

**36 GB-heures** = 10 % du stockage inclus de la plus petite formule (0,5 Go × 720 h). Un
seuil fixe, indépendant de la formule, pour que la mise en avant marche même quand la
formule est illisible. Sur les chiffres réels : exec-d en septembre (371,85) est mis en
avant ; un dépôt comme `systm-d/josephine` seul (12,9) ne l'est pas ; 7 jours ne l'est
jamais. Facturation illisible : pas de mise en avant, on ne sait pas si le stockage compte.

### 7.4 JSON

`artifact_retention_days` (entier ou `null`).

## 8. `scan --json`, forme finale d'une organisation

```json
{
  "org": "exec-d",
  "cache_bytes": 0,
  "cache_count": 0,
  "billing_readable": true,
  "plan": "team",
  "minutes_allowance": 3000,
  "billing_month": "2026-09",
  "storage_gbh": 371.85,
  "storage_allowance_gbh": 1440.0,
  "budgets_readable": true,
  "actions_budget": { "amount": 0, "blocking": true },
  "actions_sku_budgets": [],
  "artifact_retention_days": 7,
  "repos": [
    { "name": "disconnected", "cache_bytes": 0, "cache_count": 0, "storage_gbh": 359.88 }
  ]
}
```

Construit par une fonction pure `commands::scan::overview_json(&[OrgSummary], month)`,
testée sans réseau ni stdout. Stdout ne porte toujours que du JSON.

## 9. Mesures ouvertes

Rien ici n'est tranché en silence : chaque point est affiché tel quel ou documenté.

1. **Base horaire du stockage : 720 ou 744 h ?** Relevé d'exec-d en septembre 2026, en Free
   (0,5 Go) : 371,85 GB-heures, `grossAmount` = `discountAmount` = 0,1249 $, donc tout
   remisé. Sur 720 h (formule de la doc, retenue), le quota est 360 : la jauge affiche
   **103 %** d'un dépassement que GitHub n'a pas facturé. Sur 744 h, 372 : 100 %. Le prix
   relevé (0,00033602 $/GB-h) donne exactement 0,25 $/Go-mois sur 744 h. Deux explications :
   GitHub remise sur 744 h, ou le passage d'exec-d en Team le même jour a été appliqué à
   tout le mois. **À mesurer** sur une organisation Free, sans changement de formule, en fin
   d'un mois de 30 jours, au-delà de 360 GB-heures. Si c'est 744 : changer
   `billing::hours_in_month` et son test, rien d'autre.
2. **Stockage des dépôts publics** : la doc est muette. En septembre 2026, `systm-d/alertU`
   14,9, `systm-d/josephine` 12,9, `story-d/envers-du-decor` 31 GB-heures, remisés comme les
   privés. Comptés (décision 10).
3. **Quota enterprise partagé** : la doc vérifiée le 2026-09-11 ne dit pas si les 50 000
   minutes et 50 Go sont partagés entre les organisations d'un compte entreprise. L'onglet
   le suppose et dit que la jauge est un minimum (décision 6).
4. **Changement de formule en cours de mois** : `plan.name` ne donne que la formule
   actuelle, appliquée à tous les mois que l'onglet parcourt (« quota de la formule
   actuelle »).
5. **SKU d'un budget `SkuPricing`** : jamais observé ; la détection par préfixe `actions`
   est une hypothèse, signalée plutôt qu'interprétée.
6. **Budgets de portée `repository`** : non lus. L'issue ne retient que la portée
   `organization`.
7. **Budgets avec les seuls scopes du README** : la doc parle de rôle (admin, gestionnaire
   de facturation), pas de scope. Le README le documente ainsi ; le relevé du 2026-09-10
   avait été fait avec un token `gh` portant `admin:org`.
8. **`maximum_allowed_days`** : l'exemple de la doc (`days` 100, maximum 90) est
   incohérent ; seule la réponse réelle d'exec-d (90 / 400) sert de fixture.

## 10. Architecture

```
crates/bondebarras-core/src/
├─ billing.rs              + quotas par formule, stockage, budgets, seuil de rétention
├─ model.rs                OrgSummary : + plan, budgets, retention ; + ArtifactRetention
├─ scan.rs                 overview : lectures dégradables jointes
├─ api/orgs.rs             (neuf) GET /orgs/{org} → plan.name
├─ api/budgets.rs          (neuf) budgets paginés, tout échec → None
├─ api/retention.rs        (neuf) rétention, tout échec → None
├─ commands/scan.rs        overview_json pur
├─ tui/views/billing.rs    onglet : en-tête, budget, minutes, stockage, rétention, coûts, notes
├─ tui/views/gauges.rs     minutes_gauge_line prend le quota ; percent réutilisé tel quel ; cache_over_ceiling
└─ tui/views/repos.rs      ⚠ plafond de cache, ligne de détail GB-h
```

**Ce que ça ne touche pas :** `clean.rs`, `commands/clean.rs`, `safety.rs`, les paliers.
Aucune écriture : pas de `PUT`, `PATCH` ni `DELETE` nouveau.

## 11. Tests

| Cible | Vérification |
|---|---|
| #12 | rendu 60 → 200 colonnes : `6.15 $` présent, `€` absent |
| #11 quota | trois formules ; inconnue → `None` ; `None` → `None` |
| #11 API | `plan.name` lu ; champ absent → `None` ; 403 → `None` (wiremock) |
| #11 rendu | formule inconnue : **aucun `%` dans tout l'onglet**, à toutes les tailles |
| #11 chiffres réels | exec-d 2026-09, 1 004 min en Team → `33 %`, jamais `50 %` |
| #11 colonne 3 | quota 3 000 → 33 % ; `None` → pas de `%` |
| #13 somme | seules les lignes `Actions storage` en `GigabyteHours`, jamais les minutes |
| #13 classement | exec-d 2026-09 : `disconnected` 359,88, puis `ptitjardinier-app` 11,21 |
| #13 base | septembre → 720 h, juillet → 744 h (une constante échoue sur l'un des deux) |
| #13 ⚠ | 10 Gio tout rond → non ; + 1 octet → oui |
| #13 rendu | bloc stockage, base, ligne fixe, colonne 2 : visibles à toutes les largeurs |
| #14 sélection | réponse réelle d'exec-d : seul le budget `actions` retenu ; `SkuPricing` signalé |
| #14 API | 400 `Unable to get budgets.` → illisible ; `has_next_page` suivi ; entrée mal formée → illisible |
| #14 JSON | « aucun budget » et « illisible » ne produisent pas le même objet |
| #14 rendu | 0 $ bloquant, 5 $ bloquant, aucun, illisible ; alerte à 95 % bloquant, absente sans budget |
| #15 API | `days` et `maximum_allowed_days` ; 403 → `None` |
| #15 rendu | 90 j mis en avant avec 371,85 GB-h ; 7 j non ; 90 j avec 12,9 GB-h non |

Les tests de rendu passent par `tui::views::render` — la vraie disposition, donc le vrai
`Rect` du panneau — et balaient les largeurs de 60 à 200 puis les hauteurs depuis la
première qui peut montrer la ligne.

## 12. Hors périmètre

- Modifier la rétention (`PUT`) : tâche de suivi éventuelle, au palier 2 (décision 17).
- Modifier un budget : définitivement.
- Projection de fin de mois (demanderait le rapport journalier, une requête de plus).
- Supprimer depuis l'onglet Billing : il reste diagnostique.
- Réglage manuel du quota par organisation : rien n'est persisté.
