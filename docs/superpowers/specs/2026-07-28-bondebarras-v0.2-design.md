# bondebarras v0.2 — onglet Billing et CLI headless

- **Date :** 2026-07-28
- **Statut :** Design (dérivé de la spec v0.1, §3.2 / §6.2 / §8 / §10)
- **Auteur :** k@levilainpetit.dev (avec Claude Code)
- **Prérequis :** v0.1 livrée (scan deux étages, TUI orgs + drill-down, caches/artifacts/runs)

## 1. Objectif

Deux livrables indépendants qui partagent le même cœur :

1. **L'onglet Billing** — le seul angle par lequel l'axe « minutes Actions » du problème
   initial est adressable. Les minutes ne se nettoient pas rétroactivement ; tout ce qu'on
   peut faire est **dire où elles partent**.
2. **La CLI headless** — le même moteur sans TUI, pour un cron mensuel.

## 2. Ce que la v0.1 a délibérément laissé de côté

La v0.1 n'appelle **pas** l'endpoint de facturation : son étage 1 se limite à deux requêtes
par org (caches + repos). La v0.2 ajoute un troisième appel par org, et avec lui le
marqueur ⚠ que la maquette de la spec v0.1 montrait déjà dans la colonne des orgs.

## 3. La source de données

`GET /organizations/{org}/settings/billing/usage` — vérifié fonctionnel le 2026-07-28.

```json
{ "date": "2026-07-01T00:00:00Z", "product": "actions", "sku": "Actions Linux",
  "quantity": 3311.0, "unitType": "Minutes", "pricePerUnit": 0.006,
  "grossAmount": 19.866, "discountAmount": 19.866, "netAmount": 0.0,
  "organizationName": "systm-d", "repositoryName": "josephine" }
```

Un relevé est une liste plate d'« usage items », chacun portant **un mois × un repo × un
SKU**. Toute l'agrégation est à faire côté client, ce qui est une bonne nouvelle : c'est du
calcul pur, donc testable sans réseau.

### 3.1 Les trois montants

| Champ | Sens |
|---|---|
| `grossAmount` | coût brut avant remise |
| `discountAmount` | part absorbée par le forfait gratuit |
| `netAmount` | **ce qui est réellement facturé** |

D'où la jauge de la v0.2 : **couvert / facturé**, et non « % d'un quota » — l'endpoint qui
donnait le quota total est mort (410, cf. spec v0.1 §3.1).

### 3.2 Le 403 n'est pas fatal

Constaté sur `le-vilain-petit-dev` : l'endpoint renvoie **403** quand l'utilisateur n'est
pas propriétaire de l'org. L'org reste entièrement navigable pour les caches, artifacts et
runs ; seule la colonne Billing porte un **⚠** avec le motif au survol.

Corollaire de conception : `billing` est un `Option<BillingReport>` dans `OrgSummary`, pas
un `Result` propagé. Un 403 dégrade, il n'interrompt pas.

## 4. Minutes en équivalent-inclus

GitHub facture les runners à des tarifs différents mais décompte le forfait gratuit en
**minutes-équivalent Linux** :

| SKU | Multiplicateur |
|---|---|
| `Actions Linux` | ×1 |
| `Actions Windows` | ×2 |
| `Actions macOS` (toutes variantes) | ×10 |

Le forfait gratuit d'une org est de **2 000 minutes-équivalent par mois**. Un SKU inconnu
compte ×1 et est signalé plutôt qu'ignoré silencieusement — GitHub ajoute des runners
régulièrement, et un multiplicateur muet fausserait la jauge sans prévenir.

⚠️ **Les repos publics ne consomment pas le forfait.** Les usage items d'un repo public
apparaissent avec `discountAmount == grossAmount`. La jauge doit donc se calculer sur les
minutes **facturables**, sinon un projet open source ferait paraître le quota explosé.

> **Note du 2026-07-29 (revue finale de branche) :** cette règle s'est révélée fausse pour
> moitié, et la moitié fausse rendait la jauge inutile. Vérifié contre l'API réelle : un
> repo **privé** encore dans son forfait gratuit est remisé exactement comme un repo
> public — `discountAmount == grossAmount` dans les deux cas. `discountAmount` ne
> distingue donc pas « public, gratuit à vie » de « privé, couvert par le forfait » ; se
> caler sur les minutes facturables faisait disparaître **toute** consommation privée tant
> que GitHub n'avait pas commencé à facturer, précisément la plage que la jauge existe
> pour montrer. Le mockup du §5 ci-dessous et son « 818 % » en héritent : ce chiffre était
> une estimation manuelle, non reproductible par le code, et fondée sur la même hypothèse
> erronée.
>
> Remplacé par un filtre sur la **visibilité du repo** (`private`, renvoyé par
> `GET /orgs/{org}/repos` et déjà récupéré à l'étage 1) plutôt que sur les champs de
> remise. Détails et tests dans `crates/bondebarras-core/src/billing.rs` ; rapport complet
> dans `.superpowers/sdd/2026-07-29-bondebarras-v0.2/final-fix-report.md`.

## 5. L'onglet Billing

Vue strictement diagnostique : **aucune action destructive**.

```
 bondebarras · kdelfour           Orgs   [ Billing ]  Aide

 systm-d                                    juillet 2026
 ────────────────────────────────────────────────────────
 Minutes équivalent-inclus     16 369 / 2 000    ████████ 818 %
   josephine     3 311 Linux                      3 311
   claudine      6 079 Windows ×2                12 158
   claudine         90 macOS   ×10                  900

 Coûts                    brut      couvert     facturé
   actions               86.24 €     86.24 €      0.00 €
   ────────────────────────────────────────────────────
   total                 86.24 €     86.24 €      0.00 €

 ⚠ le-vilain-petit-dev : 403, vous n'êtes pas propriétaire
 ────────────────────────────────────────────────────────
 [←/→] mois   [o] org suivante   [Tab] Orgs   [q] quitter
```

Navigation par mois : le relevé couvre plusieurs mois, et comparer deux mois est ce qui
révèle une régression de CI.

## 6. La CLI headless

Le même cœur, sans terminal interactif.

```sh
bondebarras scan  --org systm-d --json
bondebarras clean --org systm-d --caches --stale-pr --yes
```

| Drapeau | Effet |
|---|---|
| `--org <nom>` | limite à une org ; absent = toutes celles visibles par le jeton |
| `--json` | sortie machine sur stdout, rien d'autre sur stdout |
| `--caches` `--artifacts` `--runs` | familles à nettoyer, cumulables |
| `--stale-pr` | restreint aux ressources marquées ⚑ |
| `--older-than <jours>` | restreint par âge |
| `--yes` | confirme sans interaction |

**Règles non négociables :**

- Sans `--yes`, `clean` affiche le plan et **ne supprime rien** (dry-run implicite).
- Le **palier 3 est refusé en headless**, sans drapeau de contournement possible. Aucune
  ressource n'y est rattachée dans le périmètre actuel — la suppression de dépôts est hors
  périmètre — mais la règle se pose ici, au moment où la surface CLI est figée, pour qu'une
  opération future ne puisse pas se glisser dans un cron par inadvertance.
- `--json` écrit **uniquement** du JSON sur stdout ; progression et erreurs vont sur stderr,
  pour qu'un `| jq` fonctionne toujours.
- Le code de sortie est `0` si tout a réussi, `1` si au moins une suppression a échoué.

## 7. Architecture

Ajouts au découpage de la v0.1 :

```
crates/bondebarras-core/src/
├─ api/billing.rs        récupération du relevé, 403 -> None
├─ billing.rs            agrégation pure : minutes, équivalents, coûts
├─ commands/clean.rs     exécution headless du nettoyage
└─ tui/views/billing.rs  rendu de l'onglet
```

`billing.rs` ne connaît ni le réseau ni ratatui : il prend un `Vec<UsageItem>` et rend des
totaux. C'est là que vivent les multiplicateurs et la distinction facturable / couvert,
donc c'est là que se concentrent les tests.

## 8. Tests

| Cible | Vérification |
|---|---|
| multiplicateurs | Linux ×1, Windows ×2, macOS ×10, SKU inconnu ×1 **et signalé** |
| repos publics | un item entièrement remisé ne compte pas dans les minutes facturables |
| agrégation | somme par mois, par repo, par produit ; mois sans données |
| 403 | rend `None`, ne remonte pas d'erreur, l'org reste navigable |
| CLI `--json` | stdout parse en JSON ; les messages de progression sont sur stderr |
| CLI sans `--yes` | rien n'est supprimé ; le plan est affiché |
| code de sortie | 1 si une suppression échoue |

## 9. Hors périmètre

- Pas d'action destructive depuis l'onglet Billing.
- Pas de prévision ni d'extrapolation de coût — on affiche ce que GitHub rapporte.
- Pas de `--json` sur `clean` en v0.2 (le TUI reste le chemin nominal du nettoyage).
