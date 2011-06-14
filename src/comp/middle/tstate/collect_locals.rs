import std::vec;
import std::vec::plus_option;

import front::ast;
import front::ast::*;
import option::*;

import middle::walk::walk_crate;
import middle::walk::walk_fn;
import middle::walk::ast_visitor;

import aux::cinit;
import aux::ninit;
import aux::npred;
import aux::cpred;
import aux::constr;
import aux::constraint;
import aux::fn_info;
import aux::crate_ctxt;
import aux::num_constraints;
import aux::constr_map;
import aux::expr_to_constr;
import aux::constraints_expr;
import aux::ann_to_def_strict;

import util::common::new_def_hash;
import util::common::uistr;
import util::common::span;
import util::common::respan;

type ctxt = rec(@mutable vec[constr] cs,
                ty::ctxt tcx);

fn collect_local(&ctxt cx, &@local loc) -> () {
    log("collect_local: pushing " + loc.node.ident);
    vec::push[constr](*cx.cs, respan(loc.span,
                                     ninit(loc.node.ident, loc.node.id)));
}

fn collect_pred(&ctxt cx, &@expr e) -> () {
    alt (e.node) {
        case (expr_check(?e, _)) {
            vec::push[constr](*cx.cs, expr_to_constr(cx.tcx, e));
        }
        // If it's a call, generate appropriate instances of the
        // call's constraints.
        case (expr_call(?operator, ?operands, ?a)) {
            for (@ast::constr c in constraints_expr(cx.tcx, operator)) {
                auto d_id = ann_to_def_strict(cx.tcx, c.node.ann);
                alt (d_id) {
                    case (def_fn(?an_id)) {
                        let constr an_occ = respan(c.span,
                              npred(c.node.path, an_id, 
                                    aux::substitute_constr_args_(cx.tcx,
                                                                 operands,
                                                                 c)));
                        vec::push[constr](*cx.cs, an_occ);
                    }
                    case (_) {
                        cx.tcx.sess.span_err(c.span,
                                             "Non-pred in constraint");
                    }
                }
            }
            // FIXME: constraints on result type
        }
        case (_) { }
    }
}

fn find_locals(&ty::ctxt tcx, &_fn f, &span sp, &ident i, &def_id d, &ann a)
    -> ctxt {
    let ctxt cx = rec(cs=@mutable vec::alloc[constr](0u), tcx=tcx);
    auto visitor = walk::default_visitor();
    visitor = rec(visit_local_pre=bind collect_local(cx,_),
                  visit_expr_pre=bind collect_pred(cx,_)
                  with visitor);
    walk_fn(visitor, f, sp, i, d, a);
    ret cx;
}

fn add_constraint(&ty::ctxt tcx, constr c, uint next, constr_map tbl)
    -> uint {
    log(aux::constraint_to_str(tcx, c) + " |-> "
        + util::common::uistr(next));
    alt (c.node) {
        case (ninit(?i, ?id)) {
            tbl.insert(id, cinit(next, c.span, id, i));
        }
        case (npred(?p, ?id, ?args)) {
            alt (tbl.find(id)) {
                case (some[constraint](?ct)) {
                    alt (ct) {
                        case (cinit(_,_,_,_)) {
                            tcx.sess.bug("add_constraint: same def_id used"
                                         + " as a variable and a pred");
                        }
                        case (cpred(_, _, ?pds)) {
                             vec::push(*pds, respan(c.span,
                              rec(args=args, bit_num=next)));
                        }
                    }
                }
                // FIXME: this suggests a cpred shouldn't really have a
                // def_id as a field...
                case (none[constraint]) {
                    tbl.insert(id, cpred(p, id,
                      @mutable [respan(c.span, rec(args=args,
                                                    bit_num=next))]));
                }
            }
        }
    }
    ret (next + 1u);
}

/* builds a table mapping each local var defined in f
   to a bit number in the precondition/postcondition vectors */
fn mk_fn_info(&crate_ctxt ccx, &_fn f, &span f_sp,
              &ident f_name, &def_id f_id, &ann a)
    -> () {
    auto res_map = @new_def_hash[constraint]();
    let uint next = 0u;
    let vec[arg] f_args = f.decl.inputs;

    /* ignore args, which we know are initialized;
       just collect locally declared vars */

    let ctxt cx = find_locals(ccx.tcx, f, f_sp, f_name, f_id, a);
    /* now we have to add bit nums for both the constraints
       and the variables... */

    for (constr c in {*cx.cs}) {
        next = add_constraint(cx.tcx, c, next, res_map);
    }
    /* add a pseudo-entry for the function's return value
       we can safely use the function's name itself for this purpose */
    add_constraint(cx.tcx, respan(f_sp, ninit(f_name, f_id)), next, res_map);
    
    auto res = rec(constrs=res_map,
                            num_constraints=vec::len(*cx.cs) + 1u,
                   cf=f.decl.cf);

    ccx.fm.insert(f_id, res);
    
    log(f_name + " has " + uistr(num_constraints(res)) + " constraints");

}

/* initializes the global fn_info_map (mapping each function ID, including
   nested locally defined functions, onto a mapping from local variable name
   to bit number) */
fn mk_f_to_fn_info(&crate_ctxt ccx, @crate c) -> () {
  let ast_visitor vars_visitor = walk::default_visitor();
  vars_visitor = rec(visit_fn_pre=bind mk_fn_info(ccx,_,_,_,_,_)
                     with vars_visitor);

  walk_crate(vars_visitor, *c);
}

//
// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C $RBUILD 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
//

