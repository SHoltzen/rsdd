use sdd::*;
use bdd::{VarLabel, Op};
use sdd_table::*;
use std::rc::Rc;
use std::collections::HashMap;
use ref_table::*;
use apply_cache::*;
use quickersort;
use btree::*;

/// generate an even vtree by splitting a variable ordering in half `num_splits`
/// times
pub fn even_split(order: &[VarLabel], num_splits: usize) -> VTree {
    if num_splits <= 0 {
        BTree::Leaf(order.to_vec())
    } else {
        let (l_s, r_s) = order.split_at(order.len() / 2);
        let l_tree = even_split(l_s, num_splits - 1);
        let r_tree = even_split(r_s, num_splits - 1);
        BTree::Node((), Box::new(l_tree), Box::new(r_tree))
    }
}

pub struct SddManager {
    /// Managers ordered by their order in a depth-first left-first traversal of
    /// the vtree
    tbl: SddTable,
    vtree: VTree,
    external_table: ExternalRefTable<SddPtr>,
    app_cache: Vec<SubTable<(SddPtr, SddPtr), SddPtr>>
}

pub enum AppResult {
    Unsat,
    Sat(ExternalRef)
}

impl SddManager {
    pub fn new(vtree: VTree) -> SddManager {
        let mut app_cache = Vec::new();
        for _ in vtree.in_order_iter() {
            app_cache.push(SubTable::new(10000));
        }
        let mut c = SddManager {
            tbl: SddTable::new(&vtree),
            vtree: vtree,
            external_table: ExternalRefTable::new(),
            app_cache: app_cache
        };
        return c;
    }


    pub fn var(&mut self, lbl: VarLabel, is_true: bool) -> ExternalRef {
        fn var_helper(
            man: &mut SddManager,
            count: u16,
            lbl: VarLabel,
            is_true: bool,
            value: bool,
            vtree: &VTree,
        ) -> (SddPtr, u16) {
            match vtree {
                &BTree::Leaf(ref lbl_vec) => {
                    if lbl_vec.iter().find(|&x| *x == lbl).is_some() {
                        let vlbl = man.tbl.sdd_to_bdd.get(&lbl).unwrap().clone();
                        let bdd_r = man.tbl.bdd_man_mut(count as usize).var(vlbl.clone(), is_true);
                        (SddPtr::new_bdd(bdd_r, count), count + 1)
                    } else {
                        let bdd_r = if value {
                            man.tbl.bdd_man(count as usize).true_ptr()
                        } else {
                            man.tbl.bdd_man(count as usize).false_ptr()
                        };
                        (SddPtr::new_bdd(bdd_r, count), count + 1)
                    }
                }
                &BTree::Node(_, ref l, ref r) if
                    l.contains_leaf(&|v: &Vec<VarLabel>| v.contains(&lbl)) => {
                    let new_cnt = count + 1;
                    let (p1, cnt_l) = var_helper(man, new_cnt, lbl, is_true, true, &l);
                    let (p2, _) = var_helper(man, new_cnt, lbl, !is_true, true, &l);
                    let (s1, cnt_r) = var_helper(man, cnt_l, lbl, is_true, true, &r);
                    let (s2, _) = var_helper(man, cnt_l, lbl, is_true, false, &r);
                    let mut r_vec = vec![(p1, s1), (p2, s2)];
                    quickersort::sort(&mut r_vec[..]);
                    r_vec.dedup();
                    let new_sdd = man.tbl.get_or_insert_sdd(
                        SddOr { nodes: r_vec },
                        count as usize,
                    );
                    (new_sdd, cnt_r)
                }
                &BTree::Node(_, ref l, ref r) => {
                    let (p, cnt_l) = var_helper(man, count + 1, lbl, is_true, true, &l);
                    let (s, cnt_r) = var_helper(man, cnt_l, lbl, is_true, value, &r);
                    let r_vec = vec![(p, s)];
                    let new_sdd = man.tbl.get_or_insert_sdd(
                        SddOr { nodes: r_vec },
                        count as usize,
                    );
                    (new_sdd, cnt_r)
                }
            }
        }
        // TODO this is gross; cloning the vtree should definitely be avoided
        let new_v = self.vtree.clone();
        let (res, _) = var_helper(self, 0, lbl, is_true, true, &new_v);
        self.external_table.gen_or_inc(res)
    }

    pub fn apply(&mut self, op: Op, a: ExternalRef, b: ExternalRef) -> ExternalRef {
        fn helper(
            man: &mut SddManager,
            order: &BTree<usize, usize>,
            is_prime: bool,
            op: Op,
            a: SddPtr,
            b: SddPtr,
        ) -> Option<SddPtr> {
            match order {
                &BTree::Leaf(c) => {
                    let a_bdd = a.as_bdd_ptr();
                    let b_bdd = b.as_bdd_ptr();
                    let r = man.tbl.bdd_man_mut(c).apply(op, a_bdd, b_bdd);
                    if r.is_false() && is_prime {
                         None
                    } else {
                        Some(SddPtr::new_bdd(r, c as u16))
                    }
                },
                &BTree::Node(c, ref l_n, ref r_n) => {
                    let v = man.app_cache[c].get((a, b));
                    match v {
                        None => {
                            let mut r : Vec<(SddPtr, SddPtr)> = Vec::with_capacity(30);
                            for (p1, s1) in man.tbl.sdd_or_panic(a) {
                                for (p2, s2) in man.tbl.sdd_or_panic(b) {
                                    let p = helper(man, l_n, true, Op::BddAnd, p1, p2);
                                    match p {
                                        None => (),
                                        Some(v) => {
                                            // unwrap, since it will never be `none`
                                            let s = helper(man, r_n, false, op, s1, s2).unwrap();
                                            r.push((v, s));
                                        }
                                    }
                                }
                            }
                            quickersort::sort(&mut r[..]);
                            r.dedup();
                            let l = r.len();
                            let new_v = man.tbl.get_or_insert_sdd(SddOr{nodes: r}, c);
                            man.app_cache[c].insert((a, b), new_v.clone());
                            if is_prime && l == 0 {
                                None
                            } else {
                                Some(new_v)
                            }
                        },
                        Some(v) => Some(v)
                    }
                }
            }
        }
        let i_a = self.external_table.into_internal(a);
        let i_b = self.external_table.into_internal(b);
        let t = self.vtree.into_order_tree();
        let r = helper(self, &t, false, op, i_a, i_b);
        match r {
            Some(r) => self.external_table.gen_or_inc(r),
            None => panic!("unsat base sdd")
        }
    }

    // pub fn eval_sdd(&self, ptr: ExternalRef, assgn: &HashMap<VarLabel, bool>) -> bool {
    //     fn helper(
    //         man: &SddManager,
    //         a: SddPtr,
    //         tree: &BTree<usize, usize>,
    //         assgn: &HashMap<VarLabel, bool>,
    //     ) -> bool {
    //         let mut labels: HashSet<VarLabel> = HashSet::new();
    //         for lbl in man.bdd_to_sdd[cnt].values() {
    //             labels.insert(lbl.clone());
    //         }
    //         let mut new_m: HashMap<VarLabel, bool> = HashMap::new();
    //         for (key, value) in assgn.iter() {
    //             if labels.contains(key) {
    //                 let translated = man.sdd_to_bdd.get(key).unwrap();
    //                 new_m.insert(*translated, *value);
    //             }
    //         }
    //         // now new_m has the correct variable mappings in it, so we can evaluate it

    //         let v = man.bdd_managers[cnt].eval_bdd(b1, &new_m);
    //         (v, cnt+1)
    //     }
    //     let i_a = self.external_table.into_internal(ptr);
    //     let (r, _) = helper(self, i_a, 0, assgn);
    //     r
    // }

    pub fn print_sdd(&self, ptr: ExternalRef) -> String {
        fn helper(man: &SddManager, ptr: SddPtr, cnt: u16) -> (String, u16) {
            if man.tbl.is_bdd(ptr) {
                let bdd_ptr = ptr.as_bdd_ptr();
                let s = man.tbl.bdd_man(cnt as usize).print_bdd(bdd_ptr);
                (s, cnt + 1)
            } else {
                let mut s = String::from("\\/");
                let mut new_cnt = 0;
                for &(ref prime, ref sub) in man.tbl.sdd_iter_or_panic(ptr) {
                    let (new_s1, cnt_l) = helper(man, prime.clone(), cnt + 1);
                    let (new_s2, cnt_r) = helper(man, sub.clone(), cnt_l);
                    new_cnt = cnt_r;
                    s.push_str(&format!("(/\\ {} {})", new_s1, new_s2));
                }
                (s, new_cnt)
            }
        }
        let int_ptr = self.external_table.into_internal(ptr);
        let (r, _) = helper(self, int_ptr, 0);
        r
    }
}


#[test]
fn make_sdd() {
    let simple_vtree = BTree::Node((),
        Box::new(BTree::Leaf(vec![VarLabel::new(0), VarLabel::new(1)])),
        Box::new(BTree::Leaf(vec![VarLabel::new(2), VarLabel::new(3)])),
    );
    let mut man = SddManager::new(simple_vtree);
    let v = man.var(VarLabel::new(2), true);
    println!("sdd: {}", man.print_sdd(v));
}


#[test]
fn sdd_simple_apply() {
    let simple_vtree = BTree::Node((),
        Box::new(BTree::Leaf(vec![VarLabel::new(0), VarLabel::new(1)])),
        Box::new(BTree::Leaf(vec![VarLabel::new(2), VarLabel::new(3)])),
    );
    let mut man = SddManager::new(simple_vtree);
    let v1 = man.var(VarLabel::new(1), true);
    let v2 = man.var(VarLabel::new(1), false);
    let v3 = man.apply(Op::BddAnd, v1, v2);
    println!("sdd1: {}", man.print_sdd(v1));
    println!("sdd2: {}", man.print_sdd(v2));
    println!("sdd: {}", man.print_sdd(v3));
}