from azul import *

css = """
    .__azul-native-label { font-size: 50px; }
"""

class DataModel:
    def __init__(self, counter):
        self.counter = counter

# model -> view
def my_layout_func(data, info):
    label = Label("{}".format(data.counter))
    button = Button("Update counter")
    # button.set_on_click(data, my_on_click)

    dom = Dom.body()
    dom.add_child(label.dom())
    dom.add_child(button.dom())

    return dom.style(Css.from_string(css))

# model <- view
def my_on_click(data, info):
    data.counter += 1;

    # tell azul to call the myLayoutFunc again
    return Update.RefreshDom

def main():
    model = DataModel(5)
    app = App(model, AppConfig(LayoutSolver.Default))
    app.run(WindowCreateOptions(my_layout_func))

main()